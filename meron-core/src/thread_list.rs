//! Shared mailbox listing for both frontends. Desktop (`messages.recent` in the
//! sidecar) and mobile (`mail.threadList` over FFI) answer the same question —
//! which cards belong in this mailbox view — and differ only in how they reach a
//! page of headers: desktop may go live over IMAP, mobile answers from the
//! encrypted cache so the list still works offline. The view's inputs, the
//! filter-to-source decision and the response shape live here so a fix for one
//! frontend is a fix for both; platform callers keep only the fetch itself and
//! whatever background sync they spawn afterwards.

use anyhow::Result;
use rusqlite::Connection;
use serde_json::{Value, json};

use crate::imap::MessageHeader;
use crate::{mail_model, rss, store};

/// Default page size when a caller does not ask for one.
pub const DEFAULT_LIMIT: u32 = 50;

/// The mailbox-view inputs, as both transports send them. Only the folder key
/// differs between the two param vocabularies ("folder" vs "folder_id").
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ThreadListQuery {
    pub folder: String,
    pub query: String,
    pub filter: String,
    pub before_cursor: Option<(i64, u32)>,
    pub limit: u32,
}

impl ThreadListQuery {
    pub fn from_params(params: &Value, folder_key: &str) -> Self {
        let folder = params
            .get(folder_key)
            .and_then(Value::as_str)
            .filter(|value| !value.is_empty())
            .map(mail_model::canon_folder)
            .unwrap_or_else(|| "INBOX".to_string());
        Self {
            folder,
            query: params
                .get("query")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .trim()
                .to_string(),
            filter: params
                .get("filter")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
            before_cursor: params
                .get("before_cursor")
                .and_then(Value::as_str)
                .and_then(parse_mail_cursor),
            limit: params
                .get("limit")
                .and_then(Value::as_u64)
                .and_then(|limit| u32::try_from(limit).ok())
                .filter(|limit| *limit > 0)
                .unwrap_or(DEFAULT_LIMIT),
        }
    }

    /// Which page the (filter, query) pair asks for.
    pub fn source(&self) -> MailSource {
        if !self.query.is_empty() {
            return MailSource::Search;
        }
        match self.filter.as_str() {
            "starred" => MailSource::Starred,
            "unread" => MailSource::Recent { unread_only: true },
            _ => MailSource::Recent { unread_only: false },
        }
    }

    /// Whether this request should also kick off a server sync: only the first
    /// page of an unfiltered, unsearched view — the other sources are answered
    /// by their own live call or are cheap local reads.
    pub fn wants_background_sync(&self) -> bool {
        self.before_cursor.is_none() && matches!(self.source(), MailSource::Recent { .. })
    }
}

/// Where a mail page comes from. Both frontends map filters to the same source;
/// what each source *reads* (live IMAP vs the local cache) is theirs to decide.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MailSource {
    /// Starred-only view, unpaginated.
    Starred,
    /// Newest-first page, cursor paged, optionally unread-only.
    Recent { unread_only: bool },
    /// Text search across the folder (plus Sent), unpaginated.
    Search,
}

/// `date:<date>:<uid>` keyset cursor, as minted by [`store::get_recent_page`].
pub fn parse_mail_cursor(cursor: &str) -> Option<(i64, u32)> {
    let rest = cursor.strip_prefix("date:")?;
    let (date, uid) = rest.split_once(':')?;
    Some((date.parse().ok()?, uid.parse().ok()?))
}

/// Feed accounts: one card per subscription, filtered like a mail folder.
pub fn rss_page(conn: &Connection, account: &str, query: &ThreadListQuery) -> Result<Value> {
    let threads = rss::recent(
        conn,
        account,
        &query.query,
        &query.filter,
        query.limit as i64,
    )?;
    let folder_unread = rss::unread_count(conn, account)?;
    Ok(json!({ "threads": threads, "folder_unread": folder_unread }))
}

/// Mail accounts: shape a fetched header page into the bridge payload.
///
/// `group` is what thread-list callers want — core grouping (subject branching,
/// root titles, accumulated unread counts) into ready cards. Other consumers
/// (mobile is always a thread list; the desktop chat view is not) keep the raw
/// rows under "messages".
pub fn mail_page(
    conn: &Connection,
    account: &str,
    folder: &str,
    mut messages: Vec<MessageHeader>,
    next_cursor: Option<String>,
    group: bool,
) -> Result<Value> {
    // Rewrite each card's identity to the correspondent so a thread shows the
    // same person/avatar in every folder (outbound copies show the recipient).
    store::apply_card_identity(conn, account, folder, &mut messages);
    let folder_unread = store::get_folder_unread(conn, account, folder)?;
    let mut out = if group {
        let draft_thread_keys = store::draft_thread_keys(conn, account)?;
        let threads =
            mail_model::thread_cards_json(conn, account, folder, messages, &draft_thread_keys)?;
        json!({ "threads": threads, "folder_unread": folder_unread })
    } else {
        json!({ "messages": serde_json::to_value(messages)?, "folder_unread": folder_unread })
    };
    if let Some(cursor) = next_cursor {
        out.as_object_mut()
            .unwrap()
            .insert("next_cursor".to_string(), Value::String(cursor));
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn filters_pick_the_same_source_for_both_frontends() {
        let query = |params: Value| ThreadListQuery::from_params(&params, "folder");
        assert_eq!(
            query(json!({"filter": "all"})).source(),
            MailSource::Recent { unread_only: false }
        );
        assert_eq!(
            query(json!({"filter": "unread"})).source(),
            MailSource::Recent { unread_only: true }
        );
        assert_eq!(
            query(json!({"filter": "starred"})).source(),
            MailSource::Starred
        );
        // A search wins over any filter: the source has no filtered variant.
        assert_eq!(
            query(json!({"filter": "starred", "query": "hello"})).source(),
            MailSource::Search
        );
        // A blank search is not a search.
        assert_eq!(
            query(json!({"query": "   "})).source(),
            MailSource::Recent { unread_only: false }
        );
    }

    #[test]
    fn params_default_to_the_inbox_and_the_default_page_size() {
        let bare = ThreadListQuery::from_params(&json!({}), "folder_id");
        assert_eq!(bare.folder, "INBOX");
        assert_eq!(bare.limit, DEFAULT_LIMIT);
        assert!(bare.before_cursor.is_none());
        assert!(bare.wants_background_sync());

        let full = ThreadListQuery::from_params(
            &json!({"folder_id": "inbox", "limit": 10, "before_cursor": "date:200:7"}),
            "folder_id",
        );
        assert_eq!(full.folder, "INBOX", "folder names are canonicalized");
        assert_eq!(full.limit, 10);
        assert_eq!(full.before_cursor, Some((200, 7)));
        assert!(
            !full.wants_background_sync(),
            "paging older never triggers a sync"
        );
    }

    #[test]
    fn only_unfiltered_first_pages_ask_for_a_sync() {
        let query = |params: Value| ThreadListQuery::from_params(&params, "folder");
        assert!(query(json!({"filter": "unread"})).wants_background_sync());
        assert!(!query(json!({"filter": "starred"})).wants_background_sync());
        assert!(!query(json!({"query": "hello"})).wants_background_sync());
    }
}

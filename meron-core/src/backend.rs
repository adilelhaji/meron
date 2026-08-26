//! Protocol-neutral mail session boundary.
//!
//! The engine's session pool and `with_*_session` combinators operate on this
//! enum rather than on `imap::Session` directly, so an account's protocol is
//! decided once at connect time and every operation dispatches through here —
//! IMAP today, EWS next. An enum (not a trait object) because the operations
//! are async and return protocol-shared types; matching keeps them statically
//! dispatched without an `async_trait` dependency.
//!
//! IMAP IDLE watchers and the `account.connect` validation path build their
//! own dedicated `imap::connect` connections and are not routed through this
//! type: IDLE is an IMAP-specific long-lived protocol state, not a pooled
//! request session.

use anyhow::Result;
use std::collections::HashSet;

use crate::imap;
use crate::parse;

pub enum Session {
    Imap(imap::Session),
    Ews(crate::exchange::EwsSession),
}

/// Establish a fresh authenticated session for `creds`. An account's protocol
/// is decided here, once per pool refill, by whether it carries an EWS
/// endpoint.
///
/// `db` is handed to the Exchange backend, which needs the store on its own
/// operations to map opaque item ids onto the uids the rest of the core
/// addresses messages by; the IMAP backend ignores it.
pub async fn connect(
    creds: &imap::Creds,
    account: &str,
    db: std::sync::Arc<std::sync::Mutex<rusqlite::Connection>>,
) -> Result<Session> {
    if creds.is_ews() {
        return Ok(Session::Ews(crate::exchange::EwsSession::new(
            crate::exchange::EwsConfig {
                url: creds.ews_url.clone(),
                username: creds.user.clone(),
                password: creds.password.clone(),
            },
            account,
            db,
        )));
    }
    Ok(Session::Imap(imap::connect(creds).await?))
}

/// Reports an operation the Exchange backend does not implement yet.
///
/// Returning an error rather than a silent no-op is deliberate: a write that
/// quietly did nothing would let the UI show a flag or a move that never
/// reached the server.
fn ews_unsupported<T>(operation: &str) -> Result<T> {
    Err(anyhow::anyhow!(
        "{operation} is not supported on Exchange accounts yet"
    ))
}

// Each method dispatches the like-named `imap::` operation; signatures are
// identical minus the session parameter. Protocol-shared types (`Folder`,
// `MessageHeader`, …) stay in `imap` until the EWS variant forces a move.
impl Session {
    pub async fn list_folders(&mut self) -> Result<Vec<imap::Folder>> {
        match self {
            Session::Imap(session) => imap::list_folders(session).await,
            Session::Ews(session) => session.list_folders().await,
        }
    }

    pub async fn create_folder(&mut self, name: &str) -> Result<()> {
        match self {
            Session::Imap(session) => imap::create_folder(session, name).await,
            Session::Ews(_) => ews_unsupported("create_folder"),
        }
    }

    pub async fn prepare_folder_delete(&mut self) -> Result<()> {
        match self {
            Session::Imap(session) => imap::prepare_folder_delete(session).await,
            Session::Ews(_) => ews_unsupported("prepare_folder_delete"),
        }
    }

    pub async fn delete_folders(&mut self, names: &[String]) -> Result<Vec<String>> {
        match self {
            Session::Imap(session) => imap::delete_folders(session, names).await,
            Session::Ews(_) => ews_unsupported("delete_folders"),
        }
    }

    pub async fn fetch_recent(&mut self, folder: &str, limit: u32) -> Result<imap::RecentBatch> {
        match self {
            Session::Imap(session) => imap::fetch_recent(session, folder, limit).await,
            Session::Ews(session) => session.fetch_recent(folder, limit).await,
        }
    }

    pub async fn search_uids(&mut self, folder: &str, query: &str) -> Result<Vec<u32>> {
        match self {
            Session::Imap(session) => imap::search_uids(session, folder, query).await,
            Session::Ews(_) => ews_unsupported("search_uids"),
        }
    }

    pub async fn list_all_uids(&mut self, folder: &str) -> Result<HashSet<u32>> {
        match self {
            Session::Imap(session) => imap::list_all_uids(session, folder).await,
            Session::Ews(_) => ews_unsupported("list_all_uids"),
        }
    }

    pub async fn search_starred_uids(&mut self, folder: &str, limit: u32) -> Result<Vec<u32>> {
        match self {
            Session::Imap(session) => imap::search_starred_uids(session, folder, limit).await,
            Session::Ews(_) => ews_unsupported("search_starred_uids"),
        }
    }

    pub async fn fetch_by_message_ids(
        &mut self,
        folder: &str,
        ids: &[String],
        media_root: &std::path::Path,
        account: &str,
    ) -> Result<Vec<imap::FetchedMessage>> {
        match self {
            Session::Imap(session) => {
                imap::fetch_by_message_ids(session, folder, ids, media_root, account).await
            }
            Session::Ews(_) => ews_unsupported("fetch_by_message_ids"),
        }
    }

    pub async fn fetch_headers_by_uid(
        &mut self,
        folder: &str,
        uids: &[u32],
    ) -> Result<Vec<imap::MessageHeader>> {
        match self {
            Session::Imap(session) => imap::fetch_headers_by_uid(session, folder, uids).await,
            Session::Ews(_) => ews_unsupported("fetch_headers_by_uid"),
        }
    }

    pub async fn sync_flags(
        &mut self,
        folder: &str,
        since_modseq: u64,
        validity_matches: bool,
    ) -> Result<imap::FlagSync> {
        match self {
            Session::Imap(session) => {
                imap::sync_flags(session, folder, since_modseq, validity_matches).await
            }
            Session::Ews(_) => ews_unsupported("sync_flags"),
        }
    }

    pub async fn read_message(
        &mut self,
        folder: &str,
        uid: u32,
        media: &parse::MediaCtx,
    ) -> Result<parse::Message> {
        match self {
            Session::Imap(session) => imap::read_message(session, folder, uid, media).await,
            Session::Ews(session) => session.read_message(folder, uid, media).await,
        }
    }

    pub async fn prepare_flag_update(&mut self, folder: &str) -> Result<()> {
        match self {
            Session::Imap(session) => imap::prepare_flag_update(session, folder).await,
            Session::Ews(session) => {
                session.prepare_flag_update(folder);
                Ok(())
            }
        }
    }

    /// Transmits a message. Only the Exchange backend implements this: IMAP
    /// accounts submit through SMTP, which is not a session operation.
    pub async fn send_mime(&mut self, raw: Vec<u8>) -> Result<()> {
        match self {
            Session::Imap(_) => Err(anyhow::anyhow!(
                "IMAP accounts send through SMTP, not the mail session"
            )),
            Session::Ews(session) => session.send_mime(raw).await,
        }
    }

    pub async fn store_seen(&mut self, uids: &[u32], seen: bool) -> Result<()> {
        match self {
            Session::Imap(session) => imap::store_seen(session, uids, seen).await,
            Session::Ews(session) => session.store_seen(uids, seen).await,
        }
    }

    pub async fn store_starred(&mut self, uids: &[u32], starred: bool) -> Result<()> {
        match self {
            Session::Imap(session) => imap::store_starred(session, uids, starred).await,
            Session::Ews(_) => ews_unsupported("store_starred"),
        }
    }

    pub async fn move_to_folder(
        &mut self,
        source_folder: &str,
        dest_folder: &str,
        uids: &[u32],
    ) -> Result<()> {
        match self {
            Session::Imap(session) => {
                imap::move_to_folder(session, source_folder, dest_folder, uids).await
            }
            Session::Ews(session) => {
                session.move_to_folder(source_folder, dest_folder, uids).await
            }
        }
    }

    pub async fn fetch_raw_messages_for_copy(
        &mut self,
        folder: &str,
        uids: &[u32],
    ) -> Result<Vec<imap::RawMessageCopy>> {
        match self {
            Session::Imap(session) => {
                imap::fetch_raw_messages_for_copy(session, folder, uids).await
            }
            Session::Ews(_) => ews_unsupported("fetch_raw_messages_for_copy"),
        }
    }

    pub async fn append_copied_message(
        &mut self,
        folder: &str,
        message: &imap::RawMessageCopy,
    ) -> Result<()> {
        match self {
            Session::Imap(session) => imap::append_copied_message(session, folder, message).await,
            Session::Ews(_) => ews_unsupported("append_copied_message"),
        }
    }

    pub async fn expunge_uids(&mut self, folder: &str, uids: &[u32]) -> Result<()> {
        match self {
            Session::Imap(session) => imap::expunge_uids(session, folder, uids).await,
            Session::Ews(session) => session.expunge_uids(folder, uids).await,
        }
    }

    pub async fn empty_folder(&mut self, folder: &str) -> Result<u32> {
        match self {
            Session::Imap(session) => imap::empty_folder(session, folder).await,
            Session::Ews(_) => ews_unsupported("empty_folder"),
        }
    }

    pub async fn append_to_sent(&mut self, folder: &str, raw: &[u8]) -> Result<()> {
        match self {
            Session::Imap(session) => imap::append_to_sent(session, folder, raw).await,
            Session::Ews(_) => ews_unsupported("append_to_sent"),
        }
    }

    pub async fn replace_draft(
        &mut self,
        folder: &str,
        raw: &[u8],
        message_id: &str,
    ) -> Result<()> {
        match self {
            Session::Imap(session) => imap::replace_draft(session, folder, raw, message_id).await,
            Session::Ews(_) => ews_unsupported("replace_draft"),
        }
    }

    pub async fn discard_draft(&mut self, folder: &str, message_id: &str) -> Result<usize> {
        match self {
            Session::Imap(session) => imap::discard_draft(session, folder, message_id).await,
            Session::Ews(_) => ews_unsupported("discard_draft"),
        }
    }

    /// The folder holding a special-use role, preferring what the server
    /// declares and falling back to the name heuristics.
    ///
    /// Derived from the folder listing rather than implemented per backend:
    /// both report the role a server assigns (IMAP through its LIST
    /// attributes, Exchange through its distinguished folder ids), and the
    /// name fallback is shared.
    async fn find_role_folder(
        &mut self,
        role: &str,
        looks_like: fn(&str) -> bool,
    ) -> Result<Option<String>> {
        let folders = self.list_folders().await?;
        let by_role = folders
            .iter()
            .find(|folder| folder.special_use.as_deref() == Some(role))
            .map(|folder| folder.name.clone());
        let by_name = folders
            .iter()
            .find(|folder| looks_like(&folder.name))
            .map(|folder| folder.name.clone());
        Ok(by_role.or(by_name))
    }

    pub async fn find_sent_folder(&mut self) -> Result<Option<String>> {
        self.find_role_folder("sent", imap::looks_like_sent).await
    }

    pub async fn find_trash_folder(&mut self) -> Result<Option<String>> {
        self.find_role_folder("trash", imap::looks_like_trash).await
    }

    pub async fn find_archive_folder(&mut self) -> Result<Option<String>> {
        self.find_role_folder("archive", imap::looks_like_archive)
            .await
    }

    pub async fn find_drafts_folder(&mut self) -> Result<Option<String>> {
        self.find_role_folder("drafts", imap::looks_like_drafts)
            .await
    }

    pub async fn fetch_full_message(
        &mut self,
        uid: u32,
        media: &parse::MediaCtx,
        peek: bool,
    ) -> Result<Option<parse::Message>> {
        match self {
            Session::Imap(session) => imap::fetch_full_message(session, uid, media, peek).await,
            Session::Ews(_) => ews_unsupported("fetch_full_message"),
        }
    }

    pub async fn fetch_bodies(
        &mut self,
        folder: &str,
        uids: &[u32],
        media_root: std::path::PathBuf,
        account: &str,
    ) -> Result<Vec<(u32, parse::Message)>> {
        match self {
            Session::Imap(session) => {
                imap::fetch_bodies(session, folder, uids, media_root, account).await
            }
            Session::Ews(session) => {
                session.fetch_bodies(folder, uids, media_root, account).await
            }
        }
    }

    pub async fn search_prefetch_uids(&mut self, folder: &str, days: u32) -> Result<Vec<u32>> {
        match self {
            Session::Imap(session) => imap::search_prefetch_uids(session, folder, days).await,
            Session::Ews(session) => session.search_prefetch_uids(folder, days),
        }
    }
}

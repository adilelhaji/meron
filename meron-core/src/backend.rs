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
}

/// Establish a fresh authenticated session for `creds`. The protocol choice
/// will branch here once accounts can be EWS; every pool refill goes through
/// this single decision point.
pub async fn connect(creds: &imap::Creds) -> Result<Session> {
    Ok(Session::Imap(imap::connect(creds).await?))
}

// Each method dispatches the like-named `imap::` operation; signatures are
// identical minus the session parameter. Protocol-shared types (`Folder`,
// `MessageHeader`, …) stay in `imap` until the EWS variant forces a move.
impl Session {
    pub async fn list_folders(&mut self) -> Result<Vec<imap::Folder>> {
        match self {
            Session::Imap(session) => imap::list_folders(session).await,
        }
    }

    pub async fn create_folder(&mut self, name: &str) -> Result<()> {
        match self {
            Session::Imap(session) => imap::create_folder(session, name).await,
        }
    }

    pub async fn prepare_folder_delete(&mut self) -> Result<()> {
        match self {
            Session::Imap(session) => imap::prepare_folder_delete(session).await,
        }
    }

    pub async fn delete_folders(&mut self, names: &[String]) -> Result<Vec<String>> {
        match self {
            Session::Imap(session) => imap::delete_folders(session, names).await,
        }
    }

    pub async fn fetch_recent(&mut self, folder: &str, limit: u32) -> Result<imap::RecentBatch> {
        match self {
            Session::Imap(session) => imap::fetch_recent(session, folder, limit).await,
        }
    }

    pub async fn search_uids(&mut self, folder: &str, query: &str) -> Result<Vec<u32>> {
        match self {
            Session::Imap(session) => imap::search_uids(session, folder, query).await,
        }
    }

    pub async fn list_all_uids(&mut self, folder: &str) -> Result<HashSet<u32>> {
        match self {
            Session::Imap(session) => imap::list_all_uids(session, folder).await,
        }
    }

    pub async fn search_starred_uids(&mut self, folder: &str, limit: u32) -> Result<Vec<u32>> {
        match self {
            Session::Imap(session) => imap::search_starred_uids(session, folder, limit).await,
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
        }
    }

    pub async fn fetch_headers_by_uid(
        &mut self,
        folder: &str,
        uids: &[u32],
    ) -> Result<Vec<imap::MessageHeader>> {
        match self {
            Session::Imap(session) => imap::fetch_headers_by_uid(session, folder, uids).await,
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
        }
    }

    pub async fn prepare_flag_update(&mut self, folder: &str) -> Result<()> {
        match self {
            Session::Imap(session) => imap::prepare_flag_update(session, folder).await,
        }
    }

    pub async fn store_seen(&mut self, uids: &[u32], seen: bool) -> Result<()> {
        match self {
            Session::Imap(session) => imap::store_seen(session, uids, seen).await,
        }
    }

    pub async fn store_starred(&mut self, uids: &[u32], starred: bool) -> Result<()> {
        match self {
            Session::Imap(session) => imap::store_starred(session, uids, starred).await,
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
        }
    }

    pub async fn append_copied_message(
        &mut self,
        folder: &str,
        message: &imap::RawMessageCopy,
    ) -> Result<()> {
        match self {
            Session::Imap(session) => imap::append_copied_message(session, folder, message).await,
        }
    }

    pub async fn expunge_uids(&mut self, folder: &str, uids: &[u32]) -> Result<()> {
        match self {
            Session::Imap(session) => imap::expunge_uids(session, folder, uids).await,
        }
    }

    pub async fn empty_folder(&mut self, folder: &str) -> Result<u32> {
        match self {
            Session::Imap(session) => imap::empty_folder(session, folder).await,
        }
    }

    pub async fn append_to_sent(&mut self, folder: &str, raw: &[u8]) -> Result<()> {
        match self {
            Session::Imap(session) => imap::append_to_sent(session, folder, raw).await,
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
        }
    }

    pub async fn discard_draft(&mut self, folder: &str, message_id: &str) -> Result<usize> {
        match self {
            Session::Imap(session) => imap::discard_draft(session, folder, message_id).await,
        }
    }

    pub async fn find_sent_folder(&mut self) -> Result<Option<String>> {
        match self {
            Session::Imap(session) => imap::find_sent_folder(session).await,
        }
    }

    pub async fn find_trash_folder(&mut self) -> Result<Option<String>> {
        match self {
            Session::Imap(session) => imap::find_trash_folder(session).await,
        }
    }

    pub async fn find_archive_folder(&mut self) -> Result<Option<String>> {
        match self {
            Session::Imap(session) => imap::find_archive_folder(session).await,
        }
    }

    pub async fn find_drafts_folder(&mut self) -> Result<Option<String>> {
        match self {
            Session::Imap(session) => imap::find_drafts_folder(session).await,
        }
    }

    pub async fn fetch_full_message(
        &mut self,
        uid: u32,
        media: &parse::MediaCtx,
        peek: bool,
    ) -> Result<Option<parse::Message>> {
        match self {
            Session::Imap(session) => imap::fetch_full_message(session, uid, media, peek).await,
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
        }
    }

    pub async fn search_prefetch_uids(&mut self, folder: &str, days: u32) -> Result<Vec<u32>> {
        match self {
            Session::Imap(session) => imap::search_prefetch_uids(session, folder, days).await,
        }
    }
}

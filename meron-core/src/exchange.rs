//! Native Microsoft Exchange Web Services (EWS) backend: SOAP/XML over HTTPS.
//!
//! Targets on-premises Exchange servers (2016/2019/Subscription Edition),
//! where EWS remains supported indefinitely. Exchange Online retires
//! third-party EWS access in October 2026 and is out of scope — cloud
//! accounts are served by the existing IMAP+OAuth path.
//!
//! XML types and operation definitions come from the `ews` crate
//! (thunderbird/ews-rs, MPL-2.0). This module owns what that crate leaves to
//! the consumer: the HTTP transport (blocking `ureq` through the shared
//! proxy-aware agent, like the RSS engine and the OAuth refresh path) and the
//! typed request/response plumbing, including surfacing per-message
//! server-side errors instead of swallowing them.

use anyhow::{bail, Context as _};
use base64::Engine as _;
use std::time::Duration;

use ::ews::create_item::CreateItem;
use ::ews::delete_item::DeleteItem;
use ::ews::find_item::{FindItem, Traversal};
use ::ews::get_folder::GetFolder;
use ::ews::get_item::GetItem;
use ::ews::move_item::MoveItem;
use ::ews::server_version::ExchangeServerVersion;
use ::ews::soap::{Envelope, Header};
use ::ews::sync_folder_hierarchy::{SyncFolderHierarchy, SyncFolderHierarchyResponseMessage};
use ::ews::sync_folder_items::{SyncFolderItems, SyncFolderItemsResponseMessage};
use ::ews::update_item::{
    ConflictResolution, ItemChange, ItemChangeDescription, ItemChangeInner, UpdateItem, Updates,
};
use ::ews::{
    BaseFolderId, BaseItemId, BaseShape, CopyMoveItemData, FolderShape, ItemId, ItemShape,
    DeleteType, Message, MessageDisposition, MimeContent, Operation, OperationResponse,
    PathToElement, RealItem, ResponseClass, View,
};

use crate::calendar::{Calendar, Event, Participant};

/// A folder or item reference as `(id, change_key)`. The change key is
/// required by write operations (Exchange rejects stale ones) and optional
/// for reads.
pub type EwsId = (String, Option<String>);

/// One EWS call covers a full sync batch, so this is generous for slow links
/// but still bounded so a wedged server cannot hang a sync indefinitely.
const HTTP_TIMEOUT: Duration = Duration::from_secs(120);

/// Connection settings for one Exchange account.
#[derive(Clone)]
pub struct EwsConfig {
    /// Full EWS endpoint URL, e.g. `https://mail.example.org/EWS/Exchange.asmx`.
    pub url: String,
    /// `user@domain` or `DOMAIN\user`, whichever the server accepts.
    pub username: String,
    pub password: String,
}

/// A blocking EWS client bound to one account.
///
/// Callers on the async side wrap calls in `spawn_blocking`, the same pattern
/// as `imap::refresh_oauth_token`.
pub struct EwsClient {
    config: EwsConfig,
}

impl EwsClient {
    pub fn new(config: EwsConfig) -> Self {
        Self { config }
    }

    /// Runs one folder-hierarchy sync round. With `sync_state = None` the
    /// server returns a `Create` change for every folder in the mailbox;
    /// with a previous round's state it returns only the delta since then.
    pub fn folder_hierarchy(
        &self,
        sync_state: Option<String>,
    ) -> anyhow::Result<SyncFolderHierarchyResponseMessage> {
        let response = self.call(SyncFolderHierarchy {
            folder_shape: FolderShape {
                base_shape: BaseShape::AllProperties,
            },
            sync_folder_id: Some(BaseFolderId::DistinguishedFolderId {
                id: "msgfolderroot".to_string(),
                change_key: None,
                mailbox: None,
            }),
            sync_state,
        })?;
        single_message(into_successes(response).context("SyncFolderHierarchy")?)
    }

    /// Runs one incremental item-sync round for a folder. With
    /// `sync_state = None` the server enumerates the folder from scratch;
    /// each round returns at most `max_changes` (1..=512) changes plus the
    /// state token for the next round.
    pub fn item_sync(
        &self,
        folder: &EwsId,
        sync_state: Option<String>,
        max_changes: u16,
    ) -> anyhow::Result<SyncFolderItemsResponseMessage> {
        let response = self.call(SyncFolderItems {
            item_shape: ItemShape {
                base_shape: BaseShape::IdOnly,
                include_mime_content: None,
                additional_properties: None,
            },
            sync_folder_id: folder_id(folder),
            sync_state,
            ignore: None,
            max_changes_returned: max_changes,
            sync_scope: None,
        })?;
        single_message(into_successes(response).context("SyncFolderItems")?)
    }

    /// Creates a calendar in the mailbox, returning its id.
    pub fn create_calendar(&self, name: &str) -> anyhow::Result<String> {
        let response = self.call(::ews::create_folder::CreateFolder {
            // Under the mailbox root, where the account's own calendars live.
            parent_folder_id: BaseFolderId::DistinguishedFolderId {
                id: "calendar".to_string(),
                change_key: None,
                mailbox: None,
            },
            folders: vec![::ews::Folder::CalendarFolder {
                folder_id: None,
                parent_folder_id: None,
                folder_class: None,
                display_name: Some(name.to_string()),
                total_count: None,
                child_folder_count: None,
                extended_property: None,
            }],
        })?;
        for message in into_successes(response).context("CreateFolder calendar")? {
            for folder in message.folders.inner {
                if let ::ews::Folder::CalendarFolder {
                    folder_id: Some(id),
                    ..
                } = folder
                {
                    return Ok(id.id);
                }
            }
        }
        anyhow::bail!("Exchange created the calendar but returned no id")
    }

    /// Renames a calendar.
    pub fn rename_calendar(&self, calendar: &EwsId, name: &str) -> anyhow::Result<()> {
        let response = self.call(::ews::update_folder::UpdateFolder {
            folder_changes: ::ews::update_folder::FolderChanges {
                folder_change: ::ews::update_folder::FolderChange {
                    folder_id: folder_id(calendar),
                    updates: ::ews::update_folder::Updates::SetFolderField {
                        field_URI: PathToElement::FieldURI {
                            field_URI: "folder:DisplayName".to_string(),
                        },
                        folder: ::ews::Folder::CalendarFolder {
                            folder_id: None,
                            parent_folder_id: None,
                            folder_class: None,
                            display_name: Some(name.to_string()),
                            total_count: None,
                            child_folder_count: None,
                            extended_property: None,
                        },
                    },
                },
            },
        })?;
        into_successes(response).context("UpdateFolder calendar")?;
        Ok(())
    }

    /// Deletes a calendar and, with it, everything on it.
    ///
    /// To Deleted Items rather than permanently: this destroys a year of
    /// someone's appointments, and a server-side recovery path is worth
    /// keeping even behind a confirmation.
    pub fn delete_calendar(&self, calendar: &EwsId) -> anyhow::Result<()> {
        let response = self.call(::ews::delete_folder::DeleteFolder {
            delete_type: DeleteType::MoveToDeletedItems,
            folder_ids: vec![folder_id(calendar)],
        })?;
        into_successes(response).context("DeleteFolder calendar")?;
        Ok(())
    }

    /// Creates an event on a calendar and returns its new id.
    pub fn create_event(&self, calendar: &EwsId, event: &Event) -> anyhow::Result<EwsId> {
        let response = self.call(CreateItem {
            message_disposition: None,
            // The server refuses to create a calendar item without this, even
            // for an appointment with no attendees. SendToNone is what keeps
            // writing something in your own calendar from mailing anyone:
            // inviting people is a deliberate act, and belongs with meeting
            // invitations rather than with saving an appointment.
            send_meeting_invitations: Some(
                ::ews::create_item::SendMeetingInvitations::SendToNone,
            ),
            saved_item_folder_id: Some(folder_id(calendar)),
            items: vec![RealItem::CalendarItem(event_item(event))],
        })?;
        let message = single_message(into_successes(response).context("CreateItem calendar")?)?;
        let created = message
            .items
            .inner
            .first()
            .and_then(|item| item.inner_message().item_id.clone())
            .context("Exchange created the event but returned no id")?;
        Ok((created.id, created.change_key))
    }

    /// Applies a changed event, field by field.
    ///
    /// EWS updates one property per change, so a full replacement is expressed
    /// as a set of field updates. `change_key` carries the version the caller
    /// read: Exchange rejects a stale one rather than silently overwriting an
    /// edit made elsewhere.
    pub fn update_event(
        &self,
        item: &EwsId,
        event: &Event,
        whole_series: bool,
    ) -> anyhow::Result<()> {
        // Each change carries exactly one property, inside an item of the
        // type being updated: a calendar field in a Message is rejected.
        let field = |uri: &str, item: Message| ItemChangeDescription::SetItemField {
            field_uri: PathToElement::FieldURI {
                field_URI: uri.to_string(),
            },
            item: RealItem::CalendarItem(item),
        };
        let mut updates = vec![
            field(
                "item:Subject",
                Message {
                    subject: Some(event.subject.clone()),
                    ..Message::default()
                },
            ),
            field(
                "calendar:Start",
                Message {
                    start: Some(ews_time(event.start)),
                    ..Message::default()
                },
            ),
            field(
                "calendar:End",
                Message {
                    end: Some(ews_time(event.end)),
                    ..Message::default()
                },
            ),
            field(
                "calendar:IsAllDayEvent",
                Message {
                    is_all_day_event: Some(event.all_day),
                    ..Message::default()
                },
            ),
            field(
                "item:Body",
                Message {
                    body: Some(::ews::Body {
                        body_type: ::ews::BodyType::Text,
                        is_truncated: None,
                        content: Some(event.description.clone()),
                    }),
                    ..Message::default()
                },
            ),
            field(
                "item:ReminderIsSet",
                Message {
                    reminder_is_set: Some(event.reminder_minutes.is_some()),
                    ..Message::default()
                },
            ),
        ];
        if let Some(minutes) = event.reminder_minutes {
            // Only sent when there is one: the minutes mean nothing on an item
            // whose reminder is off.
            updates.push(field(
                "item:ReminderMinutesBeforeStart",
                Message {
                    reminder_minutes_before_start: Some(minutes.max(0) as usize),
                    ..Message::default()
                },
            ));
        }
        if let Some(location) = &event.location {
            updates.push(field(
                "calendar:Location",
                Message {
                    location: Some(location.clone()),
                    ..Message::default()
                },
            ));
        }

        let response = self.call(UpdateItem {
            message_disposition: MessageDisposition::SaveOnly,
            // Required on a calendar update for the same reason as on create,
            // and set to notify nobody for the same reason too.
            send_meeting_invitations_or_cancellations: Some(
                ::ews::update_item::SendMeetingInvitationsOrCancellations::SendToNone,
            ),
            conflict_resolution: Some(ConflictResolution::AlwaysOverwrite),
            item_changes: vec![ItemChange {
                item_change: ItemChangeInner {
                    item_id: scoped_item_id(item, whole_series),
                    updates: Updates { inner: updates },
                },
            }],
        })?;
        into_successes(response).context("UpdateItem calendar")?;
        Ok(())
    }

    /// Deletes an event.
    /// The recurrence of the series an occurrence belongs to.
    pub fn series_rule(&self, item: &EwsId) -> anyhow::Result<Option<crate::calendar::Recurrence>> {
        let response = self.call(GetItem {
            item_shape: ItemShape {
                base_shape: BaseShape::IdOnly,
                include_mime_content: None,
                additional_properties: Some(vec![PathToElement::FieldURI {
                    field_URI: "calendar:Recurrence".to_string(),
                }]),
            },
            // The master, reached through the occurrence in hand.
            item_ids: vec![scoped_item_id(item, true)],
        })?;
        for message in into_successes(response).context("GetItem recurrence")? {
            for entry in message.items.inner {
                if let Some(rule) = entry.inner_message().recurrence.as_ref() {
                    return Ok(from_ews_recurrence(rule));
                }
            }
        }
        Ok(None)
    }

    /// Answers a meeting invitation.
    ///
    /// A response is created, not written onto the appointment: the answer is
    /// a message to the organizer, and the server updates the calendar from
    /// it. Sent and saved, because an answer nobody receives is not an answer.
    pub fn respond(&self, item: &EwsId, response: crate::calendar::Response) -> anyhow::Result<()> {
        use crate::calendar::Response;
        let reference = Message {
            reference_item_id: Some(::ews::ReferenceItemId {
                id: item.0.clone(),
                change_key: item.1.clone(),
            }),
            ..Message::default()
        };
        let answer = match response {
            Response::Accept => RealItem::AcceptItem(reference),
            Response::Tentative => RealItem::TentativelyAcceptItem(reference),
            Response::Decline => RealItem::DeclineItem(reference),
        };
        let response = self.call(CreateItem {
            message_disposition: Some(MessageDisposition::SendAndSaveCopy),
            send_meeting_invitations: None,
            saved_item_folder_id: None,
            items: vec![answer],
        })?;
        into_successes(response).context("CreateItem meeting response")?;
        Ok(())
    }

    pub fn delete_event(&self, item: &EwsId, whole_series: bool) -> anyhow::Result<()> {
        let response = self.call(DeleteItem {
            delete_type: DeleteType::MoveToDeletedItems,
            // Deleting a meeting you organize would normally notify the
            // attendees; this does not, for the same reason the others do not.
            // Cancelling a meeting properly belongs with invitations.
            send_meeting_cancellations: Some(::ews::delete_item::SendMeetingCancellations::SendToNone),
            affected_task_occurrences: None,
            suppress_read_receipts: None,
            item_ids: vec![scoped_item_id(item, whole_series)],
        })?;
        into_successes(response).context("DeleteItem calendar")?;
        Ok(())
    }

    /// Occurrences on a calendar between two instants, expanded by the server.
    pub fn calendar_view(
        &self,
        calendar: &EwsId,
        from: i64,
        to: i64,
        max: usize,
    ) -> anyhow::Result<Vec<Message>> {
        let stamp = |seconds: i64| {
            chrono::DateTime::from_timestamp(seconds, 0)
                .unwrap_or_default()
                .format("%Y-%m-%dT%H:%M:%SZ")
                .to_string()
        };
        let response = self.call(FindItem {
            traversal: Traversal::Shallow,
            item_shape: ItemShape {
                base_shape: BaseShape::IdOnly,
                include_mime_content: None,
                additional_properties: Some(event_properties()),
            },
            view: Some(View::CalendarView {
                max_entries_returned: Some(max),
                start_date: stamp(from),
                end_date: stamp(to),
            }),
            parent_folder_ids: vec![folder_id(calendar)],
        })?;
        let mut items = Vec::new();
        for message in into_successes(response).context("FindItem CalendarView")? {
            for item in message.root_folder.items.inner {
                items.push(item.inner_message().clone());
            }
        }
        Ok(items)
    }

    /// Fetches envelope fields for the given items in one call, without
    /// downloading message bodies.
    ///
    /// This is the whole reason a native backend beats bridging through
    /// IMAP: serving an envelope over IMAP semantics forces a gateway to
    /// download each full message to synthesize headers, while EWS returns
    /// exactly the properties asked for.
    pub fn fetch_envelopes(&self, items: &[EwsId]) -> anyhow::Result<Vec<EwsEnvelope>> {
        let response = self.call(GetItem {
            item_shape: ItemShape {
                base_shape: BaseShape::IdOnly,
                include_mime_content: None,
                additional_properties: Some(envelope_properties()),
            },
            item_ids: items.iter().map(item_id).collect(),
        })?;
        let mut envelopes = Vec::new();
        for message in into_successes(response).context("GetItem envelopes")? {
            for item in message.items.inner {
                envelopes.push(EwsEnvelope::from_message(item.inner_message())?);
            }
        }
        Ok(envelopes)
    }

    /// Downloads the full MIME content of the given items, decoded from the
    /// base64 the server returns.
    pub fn fetch_mime(&self, items: &[EwsId]) -> anyhow::Result<Vec<(ItemId, Vec<u8>)>> {
        let response = self.call(GetItem {
            item_shape: ItemShape {
                base_shape: BaseShape::IdOnly,
                include_mime_content: Some(true),
                additional_properties: None,
            },
            item_ids: items.iter().map(item_id).collect(),
        })?;
        let mut fetched = Vec::new();
        for message in into_successes(response).context("GetItem")? {
            for item in message.items.inner {
                // Every RealItem variant carries the same Message payload.
                let message = item.inner_message().clone();
                let id = message
                    .item_id
                    .clone()
                    .context("EWS returned a message without an item id")?;
                let mime = message
                    .mime_content
                    .as_ref()
                    .context("EWS returned a message without MIME content")?;
                let raw = base64::engine::general_purpose::STANDARD
                    .decode(mime.content.as_bytes())
                    .context("decode EWS MIME content")?;
                fetched.push((id, raw));
            }
        }
        Ok(fetched)
    }

    /// Sends a fully-formed MIME message. The server transmits it and saves
    /// the copy into Sent itself (`SendAndSaveCopy`), so no separate append
    /// is needed.
    pub fn send_mime(&self, mime: &[u8]) -> anyhow::Result<()> {
        let response = self.call(CreateItem {
            message_disposition: Some(MessageDisposition::SendAndSaveCopy),
            // Only meaningful for calendar items; a mail message has none.
            send_meeting_invitations: None,
            saved_item_folder_id: None,
            items: vec![RealItem::Message(Message {
                mime_content: Some(MimeContent {
                    character_set: None,
                    content: base64::engine::general_purpose::STANDARD.encode(mime),
                }),
                ..Message::default()
            })],
        })?;
        single_message(into_successes(response).context("CreateItem")?).map(|_| ())
    }

    /// Sets the read flag on the given items.
    pub fn set_read(&self, items: &[EwsId], is_read: bool) -> anyhow::Result<()> {
        let response = self.call(UpdateItem {
            message_disposition: MessageDisposition::SaveOnly,
            send_meeting_invitations_or_cancellations: None,
            conflict_resolution: Some(ConflictResolution::AutoResolve),
            item_changes: items
                .iter()
                .map(|item| ItemChange {
                    item_change: ItemChangeInner {
                        item_id: item_id(item),
                        updates: Updates {
                            inner: vec![ItemChangeDescription::SetItemField {
                                field_uri: PathToElement::FieldURI {
                                    field_URI: "message:IsRead".to_string(),
                                },
                                item: RealItem::Message(Message {
                                    is_read: Some(is_read),
                                    ..Message::default()
                                }),
                            }],
                        },
                    },
                })
                .collect(),
        })?;
        into_successes(response).context("UpdateItem")?;
        Ok(())
    }

    /// Moves items into another folder. The server assigns new item ids; they
    /// arrive through the destination folder's next sync round.
    pub fn move_items(&self, to_folder: &EwsId, items: &[EwsId]) -> anyhow::Result<()> {
        let response = self.call(MoveItem {
            inner: CopyMoveItemData {
                to_folder_id: folder_id(to_folder),
                item_ids: items.iter().map(item_id).collect(),
                return_new_item_ids: None,
            },
        })?;
        into_successes(response).context("MoveItem")?;
        Ok(())
    }

    /// Deletes items — to the Deleted Items folder by default, permanently
    /// when `hard` is set.
    pub fn delete_items(&self, items: &[EwsId], hard: bool) -> anyhow::Result<()> {
        let response = self.call(DeleteItem {
            delete_type: if hard {
                DeleteType::HardDelete
            } else {
                DeleteType::MoveToDeletedItems
            },
            send_meeting_cancellations: None,
            affected_task_occurrences: None,
            suppress_read_receipts: None,
            item_ids: items.iter().map(item_id).collect(),
        })?;
        into_successes(response).context("DeleteItem")?;
        Ok(())
    }

    /// Executes one EWS operation and returns its typed response body.
    pub fn call<Op: Operation>(&self, op: Op) -> anyhow::Result<Op::Response> {
        let request = build_request(op)?;
        let response = self.post_soap(&request)?;
        parse_response::<Op>(&response).inspect_err(|_| {
            // A SOAP fault names the element it rejected, but that detail is
            // not carried on the parse error; surface it for diagnosis.
            ews_debug(&format!(
                "server response: {}",
                String::from_utf8_lossy(&response).chars().take(900).collect::<String>()
            ));
        })
    }

    fn post_soap(&self, request: &[u8]) -> anyhow::Result<Vec<u8>> {
        let url = &self.config.url;
        let mut response = crate::proxy::agent()?
            .post(url)
            .header(
                "Authorization",
                &basic_auth(&self.config.username, &self.config.password),
            )
            .header("Content-Type", "text/xml; charset=utf-8")
            .config()
            .http_status_as_error(false)
            .timeout_global(Some(HTTP_TIMEOUT))
            .build()
            .send(request)
            .with_context(|| format!("EWS POST {url}"))?;
        let status = response.status().as_u16();
        if status == 401 {
            bail!("EWS authentication rejected (HTTP 401) at {url}");
        }
        // 500 is not filtered out here: EWS delivers SOAP faults with it, and
        // the fault detail parsed from the body is the useful error.
        if status != 200 && status != 500 {
            bail!("EWS request failed: HTTP {status} at {url}");
        }
        response
            .body_mut()
            .read_to_vec()
            .with_context(|| format!("read EWS response from {url}"))
    }
}

/// Serializes `op` into a complete SOAP request document.
///
/// Exchange2013_SP1 is the newest schema every supported on-premises version
/// (2016 and later) understands.
fn build_request<Op: Operation>(op: Op) -> anyhow::Result<Vec<u8>> {
    Envelope {
        headers: vec![Header::RequestServerVersion {
            version: ExchangeServerVersion::Exchange2013_SP1,
        }],
        body: op,
    }
    .as_xml_document()
    .context("serialize EWS request")
}

/// Parses a SOAP response document into the operation's typed response.
/// Envelope-level SOAP faults (schema errors, throttling, invalid state)
/// surface here as typed `ews` errors.
fn parse_response<Op: Operation>(document: &[u8]) -> anyhow::Result<Op::Response> {
    let envelope: Envelope<Op::Response> =
        Envelope::from_xml_document(document).context("parse EWS response")?;
    Ok(envelope.body)
}

/// Unwraps per-message response classes, failing loud on the first
/// server-reported error. Warnings still carry an applied result, so they
/// pass through as successes.
fn into_successes<R: OperationResponse>(response: R) -> anyhow::Result<Vec<R::Message>> {
    response
        .into_response_messages()
        .into_iter()
        .map(|message| match message {
            ResponseClass::Success(message) | ResponseClass::Warning(message) => Ok(message),
            ResponseClass::Error(error) => {
                Err(anyhow::Error::new(error).context("EWS reported an operation error"))
            }
        })
        .collect()
}

/// EWS returns one response message per requested item; operations issued for
/// a single target must produce exactly one.
fn single_message<T>(mut messages: Vec<T>) -> anyhow::Result<T> {
    if messages.len() != 1 {
        bail!("expected 1 EWS response message, got {}", messages.len());
    }
    Ok(messages.remove(0))
}

/// Trace Exchange sync decisions when `MERON_EWS_DEBUG` is set. Off by default
/// so production runs stay quiet, following `MERON_POOL_DEBUG`. Routes through
/// the logger at warning level so a diagnosis run shows it in a release build.
fn ews_debug(what: &str) {
    if std::env::var_os("MERON_EWS_DEBUG").is_some() {
        crate::mlog!(crate::log::Level::Warn, "ews", "{what}");
    }
}

/// Proves an Exchange account works before it is stored: one round trip that
/// exercises the endpoint URL, the credentials and the SOAP schema together.
///
/// The IMAP path validates by opening a session and checking the submission
/// server's certificate; neither applies here, since EWS carries mail and
/// submission over the one HTTPS endpoint.
pub async fn validate(config: EwsConfig) -> anyhow::Result<()> {
    let client = EwsClient::new(config);
    tokio::task::spawn_blocking(move || client.folder_hierarchy(None)).await??;
    Ok(())
}

/// Ceiling on one `SyncFolderItems` round, which the protocol caps at 512.
/// Envelope details for the round's items are then fetched in one further
/// call, so a folder's first sync costs two round trips per batch.
const SYNC_BATCH: u16 = 512;

/// Ceiling on one calendar window. A busy three-month view stays well inside
/// it, and a runaway range cannot pull an unbounded answer.
const EVENT_PAGE: usize = 1000;

/// An authenticated Exchange session for one account.
///
/// The EWS client is blocking (`ureq`, as everywhere else in the core), so
/// every operation hops onto a blocking thread. The store handle is needed on
/// the same path: Exchange addresses messages by opaque item ids while the
/// rest of the core addresses them by `u32` uid, and the correspondence lives
/// in the database — see [`crate::store::map_ews_item`].
pub struct EwsSession {
    client: std::sync::Arc<EwsClient>,
    account: String,
    db: std::sync::Arc<std::sync::Mutex<rusqlite::Connection>>,
    /// Folder wire name to its Exchange id, hydrated by `list_folders`.
    /// Exchange addresses folders by id, the core by name.
    folders: std::collections::HashMap<String, EwsId>,
    /// The last folder listing and when it was taken. Resolving roles (Sent,
    /// Drafts, Trash) lists the hierarchy, and a single sync resolves several,
    /// which would otherwise cost two round trips each.
    listing: Option<(std::time::Instant, Vec<crate::imap::Folder>)>,
    /// Calendar id to its Exchange reference, hydrated by `list_calendars`.
    calendars: std::collections::HashMap<String, EwsId>,
    /// Folder named by the last `prepare_flag_update`. IMAP's flag calls carry
    /// no folder because they act on the selected mailbox; EWS addresses items
    /// by id, so the session remembers what was selected to resolve them.
    selected: Option<String>,
}

impl EwsSession {
    pub fn new(
        config: EwsConfig,
        account: &str,
        db: std::sync::Arc<std::sync::Mutex<rusqlite::Connection>>,
    ) -> Self {
        Self {
            client: std::sync::Arc::new(EwsClient::new(config)),
            account: account.to_string(),
            db,
            folders: std::collections::HashMap::new(),
            listing: None,
            calendars: std::collections::HashMap::new(),
            selected: None,
        }
    }

    /// Resolves the mailbox's well-known folders to their ids and roles.
    ///
    /// Exchange names these in the mailbox's own language — an inbox is
    /// "Bandeja de entrada" on a Spanish mailbox — so they can only be
    /// identified by their distinguished ids, which are fixed strings. A
    /// mailbox that lacks one (no archive, say) simply reports an error for
    /// it, which is not a failure of the listing.
    fn distinguished_roles_with(
        client: &EwsClient,
    ) -> anyhow::Result<std::collections::HashMap<String, String>> {
        let wanted = [
            ("inbox", "inbox"),
            ("sentitems", "sent"),
            ("drafts", "drafts"),
            ("deleteditems", "trash"),
            ("junkemail", "junk"),
            ("archivemsgfolderroot", "archive"),
        ];
        let response = client.call(GetFolder {
            folder_shape: FolderShape {
                base_shape: BaseShape::IdOnly,
            },
            folder_ids: wanted
                .iter()
                .map(|(id, _)| BaseFolderId::DistinguishedFolderId {
                    id: id.to_string(),
                    change_key: None,
                    mailbox: None,
                })
                .collect(),
        })?;

        // Responses come back in request order, one per requested folder,
        // errors included — which is how a missing folder is reported.
        let mut roles = std::collections::HashMap::new();
        for ((_, role), message) in wanted.iter().zip(response.into_response_messages()) {
            let (ResponseClass::Success(message) | ResponseClass::Warning(message)) = message
            else {
                continue;
            };
            for folder in message.folders.inner {
                if let ::ews::Folder::Folder {
                    folder_id: Some(id),
                    ..
                } = folder
                {
                    roles.insert(id.id, role.to_string());
                }
            }
        }
        Ok(roles)
    }

    /// Lists the mailbox's folders, refreshing the name-to-id map every call.
    ///
    /// The hierarchy is re-enumerated from scratch rather than resumed from a
    /// stored sync state: the map is per-session and must be complete, and a
    /// delta round would only report what changed since another session.
    pub async fn list_folders(&mut self) -> anyhow::Result<Vec<crate::imap::Folder>> {
        // Short-lived so a folder created elsewhere still appears promptly,
        // long enough that one sync does not re-list for every role it
        // resolves.
        const LISTING_TTL: std::time::Duration = std::time::Duration::from_secs(60);
        if let Some((taken, folders)) = self.listing.as_ref()
            && taken.elapsed() < LISTING_TTL
        {
            return Ok(folders.to_vec());
        }
        let listing_started = std::time::Instant::now();
        let client = self.client.clone();
        let round =
            tokio::task::spawn_blocking(move || client.folder_hierarchy(None)).await??;
        let roles = {
            let client = self.client.clone();
            tokio::task::spawn_blocking(move || {
                EwsSession::distinguished_roles_with(&client)
            })
            .await??
        };

        let mut folders = Vec::new();
        self.folders.clear();
        for change in round.changes.inner {
            let ::ews::sync_folder_hierarchy::Change::Create { folder } = change else {
                // A from-scratch round reports every folder as a creation;
                // updates and deletions only appear in delta rounds.
                continue;
            };
            // Only generic mail folders: a mailbox's calendar, contacts, tasks
            // and search folders are separate Exchange folder classes and are
            // not mail the client can list, open or sync.
            let ::ews::Folder::Folder {
                folder_id,
                display_name,
                unread_count,
                folder_class,
                ..
            } = folder
            else {
                continue;
            };
            if !holds_mail(folder_class.as_ref()) {
                continue;
            }
            let (Some(id), Some(name)) = (folder_id.as_ref(), display_name.as_ref()) else {
                continue;
            };
            let special_use = roles.get(&id.id).cloned();
            // The core addresses the inbox as "INBOX" throughout — IMAP
            // mandates that name, and every default in the request path
            // assumes it. Exchange has no such convention, so the wire name
            // is normalised while the mailbox's own name stays on display.
            let wire_name = if special_use.as_deref() == Some("inbox") {
                "INBOX".to_string()
            } else {
                name.clone()
            };
            self.folders
                .insert(wire_name.clone(), (id.id.clone(), id.change_key.clone()));
            folders.push(crate::imap::Folder {
                name: wire_name,
                // Exchange folder names are already Unicode; there is no
                // modified-UTF-7 layer to decode as there is on IMAP.
                display_name: name.clone(),
                delimiter: Some("/".to_string()),
                unread: unread_count.unwrap_or_default(),
                special_use,
                role: String::new(),
            });
        }
        ews_debug(&format!(
            "listed {} folders in {}ms",
            folders.len(),
            listing_started.elapsed().as_millis()
        ));
        self.listing = Some((std::time::Instant::now(), folders.clone()));
        Ok(folders)
    }

    /// Brings a folder up to date and returns its most recent messages.
    ///
    /// The first call enumerates the folder and numbers every item; later
    /// calls resume from the server's sync state and see only what changed,
    /// which is what keeps a large folder from being re-listed on every sync.
    /// Envelopes are then fetched for the newest `limit` messages known
    /// locally — the set the caller asked for, whether or not any of them
    /// changed this round.
    pub async fn fetch_recent(
        &mut self,
        folder: &str,
        limit: u32,
    ) -> anyhow::Result<crate::imap::RecentBatch> {
        let folder_ref = self.folder_ref(folder).await?;
        let mut state = self.with_store(|conn, account| {
            crate::store::get_folder_sync_state(conn, account, folder)
        })?;

        // Rounds are capped server-side, so a first enumeration takes several.
        loop {
            let client = self.client.clone();
            let target = folder_ref.clone();
            let resumed = state.clone();
            let round = tokio::task::spawn_blocking(move || {
                client.item_sync(&target, resumed, SYNC_BATCH)
            })
            .await??;
            let changes = round.changes.inner.len();
            self.apply_changes(folder, round.changes.inner)?;
            state = Some(round.sync_state.clone());
            self.with_store(|conn, account| {
                crate::store::set_folder_sync_state(conn, account, folder, &round.sync_state)
            })?;
            ews_debug(&format!(
                "item sync {folder}: {changes} changes applied, last_in_range={}",
                round.includes_last_item_in_range
            ));
            if round.includes_last_item_in_range || changes == 0 {
                break;
            }
        }

        let uids = self.with_store(|conn, account| {
            crate::store::newest_ews_uids(conn, account, folder, limit)
        })?;
        let items = self.items_for_uids(folder, &uids)?;
        let envelopes = if items.is_empty() {
            Vec::new()
        } else {
            let client = self.client.clone();
            let wanted = items.clone();
            match tokio::task::spawn_blocking(move || client.fetch_envelopes(&wanted)).await? {
                Ok(envelopes) => envelopes,
                Err(err) => {
                    ews_debug(&format!("{folder}: envelope fetch failed: {err:#}"));
                    return Err(err);
                }
            }
        };

        let messages = self.assign_uids(folder, envelopes)?;
        Ok(crate::imap::RecentBatch {
            // Exchange has no UIDVALIDITY: item ids are stable for the life of
            // the item, so the cache is never invalidated wholesale the way an
            // IMAP UIDVALIDITY bump does. A fixed 1 keeps the stored value
            // meaningful ("never reset") instead of fabricating a changing one.
            uidvalidity: 1,
            uid_next: uids.iter().copied().max().unwrap_or(0) + 1,
            messages,
        })
    }

    /// Applies one sync round to the local mapping: new items are numbered in
    /// server order, deleted ones lose their mapping, and read-flag changes
    /// are written straight to the cached rows — Exchange reports those here,
    /// so the flag sync the IMAP path uses is not needed.
    fn apply_changes(
        &self,
        folder: &str,
        changes: Vec<::ews::sync_folder_items::Change>,
    ) -> anyhow::Result<()> {
        use ::ews::sync_folder_items::Change;
        let mut conn = self
            .db
            .lock()
            .map_err(|_| anyhow::anyhow!("store lock poisoned"))?;
        // One transaction for the batch: a round can carry hundreds of
        // changes, and a statement each would dominate the sync.
        let tx = conn.transaction()?;
        for change in changes {
            match change {
                Change::Create { item } | Change::Update { item } => {
                    if let Some(id) = item.inner_message().item_id.as_ref() {
                        crate::store::map_ews_item(
                            &tx,
                            &self.account,
                            folder,
                            &id.id,
                            id.change_key.as_deref(),
                        )?;
                    }
                }
                Change::Delete { item_id } => {
                    crate::store::forget_ews_item(&tx, &self.account, folder, &item_id.id)?;
                }
                Change::ReadFlagChange { item_id, is_read } => {
                    if let Some(uid) =
                        crate::store::uid_for_ews_item(&tx, &self.account, folder, &item_id.id)?
                    {
                        crate::store::update_message_seen(
                            &tx,
                            &self.account,
                            folder,
                            uid,
                            is_read,
                        )?;
                    }
                }
            }
        }
        tx.commit()?;
        Ok(())
    }

    /// Runs `op` against the store with this session's account.
    fn with_store<T>(
        &self,
        op: impl FnOnce(&rusqlite::Connection, &str) -> anyhow::Result<T>,
    ) -> anyhow::Result<T> {
        let conn = self
            .db
            .lock()
            .map_err(|_| anyhow::anyhow!("store lock poisoned"))?;
        op(&conn, &self.account)
    }

    /// Downloads one message and parses it into the renderable form.
    ///
    /// Reading a message must not mark it read — the explicit mark-read path
    /// does that — and EWS `GetItem` has no read side effect, so this is the
    /// equivalent of IMAP's `BODY.PEEK`.
    pub async fn read_message(
        &mut self,
        folder: &str,
        uid: u32,
        media: &crate::parse::MediaCtx,
    ) -> anyhow::Result<crate::parse::Message> {
        let mut fetched = self.fetch_raw(folder, &[uid]).await?;
        let (_, raw) = fetched
            .pop()
            .with_context(|| format!("message uid {uid} not found in {folder}"))?;
        Ok(crate::parse::parse_message(&raw, Some(media)))
    }

    /// Downloads and parses a batch of messages, one call for the batch.
    pub async fn fetch_bodies(
        &mut self,
        folder: &str,
        uids: &[u32],
        media_root: std::path::PathBuf,
        account: &str,
    ) -> anyhow::Result<Vec<(u32, crate::parse::Message)>> {
        let fetched = self.fetch_raw(folder, uids).await?;
        Ok(fetched
            .into_iter()
            .map(|(uid, raw)| {
                let media = crate::parse::MediaCtx {
                    root: media_root.clone(),
                    account: account.to_string(),
                    folder: folder.to_string(),
                    uid,
                };
                (uid, crate::parse::parse_message(&raw, Some(&media)))
            })
            .collect())
    }

    /// Transmits a fully-formed MIME message.
    ///
    /// Exchange sends it and files the Sent copy in one call, so unlike the
    /// SMTP path there is no separate append afterwards.
    pub async fn send_mime(&mut self, raw: Vec<u8>) -> anyhow::Result<()> {
        let client = self.client.clone();
        tokio::task::spawn_blocking(move || client.send_mime(&raw)).await?
    }

    /// Records the folder a following flag update applies to.
    ///
    /// The IMAP backend uses this preflight to SELECT the mailbox and to let a
    /// dead pooled connection be replaced before any write reaches the server.
    /// EWS is stateless per call, so this only has to remember the folder.
    pub fn prepare_flag_update(&mut self, folder: &str) {
        self.selected = Some(folder.to_string());
    }

    /// Sets or clears the read flag on messages in the folder named by the
    /// preceding [`Self::prepare_flag_update`].
    pub async fn store_seen(&mut self, uids: &[u32], seen: bool) -> anyhow::Result<()> {
        let folder = self
            .selected
            .clone()
            .context("no folder selected for the flag update")?;
        let items = self.items_for_uids(&folder, uids)?;
        if items.is_empty() {
            return Ok(());
        }
        let client = self.client.clone();
        tokio::task::spawn_blocking(move || client.set_read(&items, seen)).await?
    }

    /// Moves messages between folders. Exchange reissues item ids on move, so
    /// the source mappings are dropped; the destination's next sync mints new
    /// uids for the arrivals.
    pub async fn move_to_folder(
        &mut self,
        source_folder: &str,
        dest_folder: &str,
        uids: &[u32],
    ) -> anyhow::Result<()> {
        let started = std::time::Instant::now();
        let destination = self.folder_ref(dest_folder).await?;
        ews_debug(&format!(
            "move: resolved {dest_folder} in {}ms",
            started.elapsed().as_millis()
        ));
        let items = self.items_for_uids(source_folder, uids)?;
        if items.is_empty() {
            return Ok(());
        }
        let client = self.client.clone();
        let moved = items.clone();
        tokio::task::spawn_blocking(move || client.move_items(&destination, &moved)).await??;
        ews_debug(&format!(
            "move: {} item(s) to {dest_folder} in {}ms total",
            items.len(),
            started.elapsed().as_millis()
        ));
        self.forget_items(source_folder, &items)
    }

    /// Deletes messages to Deleted Items, the closest equivalent of the
    /// IMAP delete-and-expunge the callers of this perform.
    pub async fn expunge_uids(&mut self, folder: &str, uids: &[u32]) -> anyhow::Result<()> {
        let items = self.items_for_uids(folder, uids)?;
        if items.is_empty() {
            return Ok(());
        }
        let client = self.client.clone();
        let deleted = items.clone();
        tokio::task::spawn_blocking(move || client.delete_items(&deleted, false)).await??;
        self.forget_items(folder, &items)
    }

    /// Fetches raw MIME for a set of uids, pairing each result back to the uid
    /// its item maps to.
    async fn fetch_raw(
        &mut self,
        folder: &str,
        uids: &[u32],
    ) -> anyhow::Result<Vec<(u32, Vec<u8>)>> {
        let items = self.items_for_uids(folder, uids)?;
        if items.is_empty() {
            return Ok(Vec::new());
        }
        let client = self.client.clone();
        let wanted = items.clone();
        let fetched =
            tokio::task::spawn_blocking(move || client.fetch_mime(&wanted)).await??;

        // Map results back by item id: EWS does not promise response order.
        let uid_of: std::collections::HashMap<&str, u32> = items
            .iter()
            .zip(uids.iter().copied())
            .map(|((id, _), uid)| (id.as_str(), uid))
            .collect();
        Ok(fetched
            .into_iter()
            .filter_map(|(id, raw)| uid_of.get(id.id.as_str()).map(|uid| (*uid, raw)))
            .collect())
    }

    /// Resolves uids to their Exchange items, skipping any the map does not
    /// know — an unmapped uid is one this account never synced.
    fn items_for_uids(&self, folder: &str, uids: &[u32]) -> anyhow::Result<Vec<EwsId>> {
        let conn = self
            .db
            .lock()
            .map_err(|_| anyhow::anyhow!("store lock poisoned"))?;
        let mut items = Vec::new();
        for uid in uids {
            if let Some(item) = crate::store::ews_item_for_uid(&conn, &self.account, folder, *uid)? {
                items.push(item);
            }
        }
        Ok(items)
    }

    /// Drops mappings for items that left the folder, so their uids are not
    /// resolved again.
    fn forget_items(&self, folder: &str, items: &[EwsId]) -> anyhow::Result<()> {
        let conn = self
            .db
            .lock()
            .map_err(|_| anyhow::anyhow!("store lock poisoned"))?;
        for (id, _) in items {
            crate::store::forget_ews_item(&conn, &self.account, folder, id)?;
        }
        Ok(())
    }

    /// The account's calendars.
    ///
    /// The hierarchy sync that lists mail folders sees these too — they are
    /// simply a different folder class — so this reuses it rather than paying
    /// a second enumeration.
    pub async fn list_calendars(&mut self) -> anyhow::Result<Vec<Calendar>> {
        let client = self.client.clone();
        let round = tokio::task::spawn_blocking(move || client.folder_hierarchy(None)).await??;
        let default = {
            let client = self.client.clone();
            tokio::task::spawn_blocking(move || {
                EwsSession::distinguished_folder(&client, "calendar")
            })
            .await??
        };
        // Removing a calendar moves it to Deleted Items rather than destroying
        // it, so the folder outlives the removal and the hierarchy still
        // reports it. Listing it again would undo the removal the user asked
        // for, so anything sitting in the wastebasket is not a calendar of
        // theirs any more.
        let deleted_items = {
            let client = self.client.clone();
            tokio::task::spawn_blocking(move || {
                EwsSession::distinguished_folder(&client, "deleteditems")
            })
            .await??
        };

        let mut calendars = Vec::new();
        for change in round.changes.inner {
            let ::ews::sync_folder_hierarchy::Change::Create { folder } = change else {
                continue;
            };
            let ::ews::Folder::CalendarFolder {
                folder_id: Some(id),
                display_name: Some(name),
                parent_folder_id,
                ..
            } = folder
            else {
                continue;
            };
            if is_discarded(deleted_items.as_deref(), parent_folder_id.as_ref()) {
                continue;
            }
            let is_default = default.as_deref() == Some(id.id.as_str());
            self.calendars
                .insert(id.id.clone(), (id.id.clone(), id.change_key.clone()));
            calendars.push(Calendar {
                id: id.id,
                name,
                is_default,
                // The server has no opinion on either: both are this client's
                // to decide and are preserved across syncs by the store.
                enabled: true,
                color: None,
                kind: crate::calendar::CalendarKind::Account,
                url: None,
                // Exchange calendars are writable; a read-only one would be a
                // shared calendar, which this backend does not list yet.
                read_only: false,
                synced_at: 0,
            });
        }
        Ok(calendars)
    }

    /// The occurrences on one calendar between two instants.
    ///
    /// A `CalendarView` is what makes the server expand recurring series into
    /// discrete occurrences, so nothing here interprets a recurrence rule.
    pub async fn events_in_window(
        &mut self,
        calendar_id: &str,
        from: i64,
        to: i64,
    ) -> anyhow::Result<Vec<Event>> {
        let reference = self.calendar_ref(calendar_id).await?;
        let client = self.client.clone();
        let owner = calendar_id.to_string();
        let items = tokio::task::spawn_blocking(move || {
            client.calendar_view(&reference, from, to, EVENT_PAGE)
        })
        .await??;
        ews_debug(&format!(
            "calendar {owner}: {} occurrence(s) in window",
            items.len()
        ));
        items
            .iter()
            .map(|item| to_event(item, &owner))
            .collect::<anyhow::Result<Vec<_>>>()
    }

    /// Creates an event on a calendar, returning it with the id the server
    /// assigned.
    pub async fn create_event(&mut self, event: &Event) -> anyhow::Result<Event> {
        let calendar = self.calendar_ref(&event.calendar_id).await?;
        let client = self.client.clone();
        let payload = event.clone();
        let (id, change_key) =
            tokio::task::spawn_blocking(move || client.create_event(&calendar, &payload)).await??;
        Ok(Event {
            id,
            change_key,
            ..event.clone()
        })
    }

    /// Applies a changed event, to the occurrence alone or to its whole series.
    pub async fn update_event(&mut self, event: &Event, whole_series: bool) -> anyhow::Result<()> {
        let item = (event.id.clone(), event.change_key.clone());
        let client = self.client.clone();
        let payload = event.clone();
        tokio::task::spawn_blocking(move || client.update_event(&item, &payload, whole_series))
            .await?
    }

    /// Deletes an event, or the whole series it belongs to.
    pub async fn delete_event(
        &mut self,
        event_id: &str,
        change_key: Option<&str>,
        whole_series: bool,
    ) -> anyhow::Result<()> {
        let item = (event_id.to_string(), change_key.map(str::to_string));
        let client = self.client.clone();
        tokio::task::spawn_blocking(move || client.delete_event(&item, whole_series)).await?
    }

    pub async fn create_calendar(&mut self, name: &str) -> anyhow::Result<String> {
        let client = self.client.clone();
        let name = name.to_string();
        let id = tokio::task::spawn_blocking(move || client.create_calendar(&name)).await??;
        // The new calendar is not in the cached listing yet.
        self.calendars.clear();
        self.listing = None;
        Ok(id)
    }

    pub async fn rename_calendar(&mut self, calendar_id: &str, name: &str) -> anyhow::Result<()> {
        let reference = self.calendar_ref(calendar_id).await?;
        let client = self.client.clone();
        let name = name.to_string();
        tokio::task::spawn_blocking(move || client.rename_calendar(&reference, &name)).await??;
        self.listing = None;
        Ok(())
    }

    pub async fn delete_calendar(&mut self, calendar_id: &str) -> anyhow::Result<()> {
        let Some(reference) = self.calendar_ref_opt(calendar_id).await? else {
            // The server does not have it: removed from here before, or from
            // another client. What was asked for is already true, and failing
            // would leave the user a calendar they cannot get rid of.
            self.calendars.remove(calendar_id);
            self.listing = None;
            return Ok(());
        };
        let client = self.client.clone();
        tokio::task::spawn_blocking(move || client.delete_calendar(&reference)).await??;
        self.calendars.remove(calendar_id);
        self.listing = None;
        Ok(())
    }

    /// Answers a meeting invitation.
    pub async fn respond(
        &mut self,
        event_id: &str,
        change_key: Option<&str>,
        response: crate::calendar::Response,
    ) -> anyhow::Result<()> {
        let item = (event_id.to_string(), change_key.map(str::to_string));
        let client = self.client.clone();
        tokio::task::spawn_blocking(move || client.respond(&item, response)).await?
    }

    /// The rule behind a series, read from the master an occurrence belongs to.
    ///
    /// Fetched on demand rather than stored: the store keeps occurrences, and
    /// a second copy of the rule would be a second truth to go stale. One
    /// request, when a reader actually opens a repeating event.
    pub async fn series_rule(
        &mut self,
        event_id: &str,
        change_key: Option<&str>,
    ) -> anyhow::Result<Option<crate::calendar::Recurrence>> {
        let item = (event_id.to_string(), change_key.map(str::to_string));
        let client = self.client.clone();
        tokio::task::spawn_blocking(move || client.series_rule(&item)).await?
    }

    /// The Exchange reference for a calendar, listing them if needed.
    async fn calendar_ref(&mut self, calendar_id: &str) -> anyhow::Result<EwsId> {
        self.calendar_ref_opt(calendar_id)
            .await?
            .with_context(|| format!("no Exchange calendar {calendar_id}"))
    }

    /// The Exchange reference for a calendar, or `None` when the server has no
    /// such calendar. A failure to reach the server is an error, not a `None`:
    /// the two mean different things to a caller deciding whether something
    /// still exists.
    async fn calendar_ref_opt(&mut self, calendar_id: &str) -> anyhow::Result<Option<EwsId>> {
        if let Some(reference) = self.calendars.get(calendar_id) {
            return Ok(Some(reference.clone()));
        }
        self.list_calendars().await?;
        Ok(self.calendars.get(calendar_id).cloned())
    }

    /// The id of a well-known folder, or `None` when the mailbox has none.
    fn distinguished_folder(client: &EwsClient, id: &str) -> anyhow::Result<Option<String>> {
        let response = client.call(GetFolder {
            folder_shape: FolderShape {
                base_shape: BaseShape::IdOnly,
            },
            folder_ids: vec![BaseFolderId::DistinguishedFolderId {
                id: id.to_string(),
                change_key: None,
                mailbox: None,
            }],
        })?;
        for message in response.into_response_messages() {
            let (ResponseClass::Success(message) | ResponseClass::Warning(message)) = message
            else {
                continue;
            };
            for folder in message.folders.inner {
                if let ::ews::Folder::CalendarFolder {
                    folder_id: Some(id),
                    ..
                }
                | ::ews::Folder::Folder {
                    folder_id: Some(id),
                    ..
                } = folder
                {
                    return Ok(Some(id.id));
                }
            }
        }
        Ok(None)
    }

    /// The folder holding a role, answered from the store when it can be.
    ///
    /// Listing the hierarchy costs two round trips and is cached per session,
    /// but the pool holds several sessions, so a role lookup that always
    /// listed would re-list on each of them — which is what made a delete take
    /// over a second of folder listings after the message had already moved.
    /// The store's folder rows carry the roles the last sync recorded, so the
    /// answer is usually local; the listing is the fallback for an account
    /// whose folders have not been synced yet.
    pub async fn role_folder(&mut self, role: &str) -> anyhow::Result<Option<String>> {
        let cached = self.with_store(|conn, account| {
            crate::store::folder_for_role(conn, account, role).map_err(Into::into)
        })?;
        if cached.is_some() {
            return Ok(cached);
        }
        let folders = self.list_folders().await?;
        Ok(folders
            .into_iter()
            .find(|folder| folder.special_use.as_deref() == Some(role))
            .map(|folder| folder.name))
    }

    /// Unread messages in `folder` from the last `days`, for body prefetch.
    ///
    /// The IMAP backend asks the server; EWS offers no restricted search short
    /// of hand-written SOAP, so this answers from the cache the sync just
    /// refreshed — which is the set a prefetch can act on regardless.
    pub fn search_prefetch_uids(&mut self, folder: &str, days: u32) -> anyhow::Result<Vec<u32>> {
        let since = chrono::Utc::now().timestamp() - (days as i64) * 24 * 60 * 60;
        let conn = self
            .db
            .lock()
            .map_err(|_| anyhow::anyhow!("store lock poisoned"))?;
        crate::store::cached_unseen_uids_since(&conn, &self.account, folder, since)
    }

    /// Resolves each envelope's item to its local uid, minting uids for items
    /// seen for the first time, and projects them onto message headers.
    fn assign_uids(
        &self,
        folder: &str,
        envelopes: Vec<EwsEnvelope>,
    ) -> anyhow::Result<Vec<crate::imap::MessageHeader>> {
        let conn = self
            .db
            .lock()
            .map_err(|_| anyhow::anyhow!("store lock poisoned"))?;
        envelopes
            .into_iter()
            .map(|envelope| {
                let uid = crate::store::map_ews_item(
                    &conn,
                    &self.account,
                    folder,
                    &envelope.item_id,
                    envelope.change_key.as_deref(),
                )?;
                Ok(envelope.into_header(uid, folder))
            })
            .collect()
    }

    /// The Exchange id for a folder name, listing the hierarchy once if the
    /// map has not been hydrated yet.
    async fn folder_ref(&mut self, folder: &str) -> anyhow::Result<EwsId> {
        if let Some(reference) = self.folders.get(folder) {
            return Ok(reference.clone());
        }
        self.list_folders().await?;
        self.folders
            .get(folder)
            .cloned()
            .with_context(|| format!("no Exchange folder named {folder}"))
    }
}

/// The container class Exchange gives a folder that holds mail.
///
/// A mailbox mixes real mail folders with housekeeping ones — journal,
/// conversation settings, quick steps, sync issues — that share the generic
/// `Folder` type and differ only by this class. Filtering on it rather than on
/// folder names is what keeps the rule working on a mailbox in any language:
/// the class is a fixed string, the display names are localised.
const MAIL_FOLDER_CLASS: &str = "IPF.Note";

/// Whether a folder holds mail a client should show.
///
/// A folder whose class the server did not return is kept: an unlabelled
/// folder is more likely a mail folder the shape omitted than one of the
/// handful of housekeeping folders, and hiding real mail is the worse error.
fn holds_mail(folder_class: Option<&String>) -> bool {
    match folder_class {
        None => true,
        Some(class) => class == MAIL_FOLDER_CLASS,
    }
}

// ---- Calendar ---------------------------------------------------------------

/// The properties a calendar fetch asks for. Every one of these was verified
/// against a live server; `message:InReplyTo` taught us that a single
/// unsupported name costs the whole request.
fn event_properties() -> Vec<PathToElement> {
    [
        "item:Subject",
        "calendar:Start",
        "calendar:End",
        "calendar:IsAllDayEvent",
        "calendar:Location",
        "calendar:IsRecurring",
        "calendar:UID",
        "item:Body",
        "item:ReminderIsSet",
        "item:ReminderMinutesBeforeStart",
        "calendar:IsCancelled",
        "calendar:LegacyFreeBusyStatus",
        "calendar:MyResponseType",
        "calendar:Organizer",
        "calendar:RequiredAttendees",
        "calendar:OptionalAttendees",
    ]
    .into_iter()
    .map(|uri| PathToElement::FieldURI {
        field_URI: uri.to_string(),
    })
    .collect()
}

/// The item a create or update writes.
///
/// Only what the caller can actually set: whether an event recurs, is
/// cancelled or is a meeting are the server's to decide, and sending them
/// would be claiming something we do not know.
fn event_item(event: &Event) -> Message {
    Message {
        subject: Some(event.subject.clone()),
        start: Some(ews_time(event.start)),
        end: Some(ews_time(event.end)),
        is_all_day_event: Some(event.all_day),
        location: event.location.clone(),
        body: (!event.description.is_empty()).then(|| ::ews::Body {
            body_type: ::ews::BodyType::Text,
            is_truncated: None,
            content: Some(event.description.clone()),
        }),
        reminder_is_set: Some(event.reminder_minutes.is_some()),
        reminder_minutes_before_start: event
            .reminder_minutes
            .map(|minutes| minutes.max(0) as usize),
        recurrence: event
            .recurrence
            .as_ref()
            .and_then(|rule| ews_recurrence(rule, event.start)),
        ..Message::default()
    }
}

/// The Exchange recurrence for a rule, anchored on the event's own start.
///
/// Only sent when creating a series. The pattern says when it falls and the
/// range how long it runs; the element requires exactly one of each, in that
/// order, which is why they are built together here rather than separately.
fn ews_recurrence(rule: &crate::calendar::Recurrence, start: i64) -> Option<::ews::Recurrence> {
    use crate::calendar::{Frequency, Recurrence as Rule};
    let start_at = chrono::DateTime::from_timestamp(start, 0)?;
    let start_date = start_at.format("%Y-%m-%d").to_string();
    let interval = rule.interval.max(1);

    let pattern = match rule.freq {
        Frequency::Daily => ::ews::RecurrencePattern::DailyRecurrence { interval },
        Frequency::Weekly => ::ews::RecurrencePattern::WeeklyRecurrence {
            interval,
            days_of_week: rule
                .days_or_start(start)
                .into_iter()
                .map(|day| Rule::EWS_DAYS[day as usize])
                .collect::<Vec<_>>()
                .join(" "),
        },
        Frequency::Monthly => ::ews::RecurrencePattern::AbsoluteMonthlyRecurrence {
            interval,
            day_of_month: chrono::Datelike::day(&start_at) as u8,
        },
        Frequency::Yearly => ::ews::RecurrencePattern::AbsoluteYearlyRecurrence {
            day_of_month: chrono::Datelike::day(&start_at) as u8,
            month: start_at.format("%B").to_string(),
        },
    };

    // A date the reader picked wins over a count, since it is the more
    // deliberate of the two answers to "for how long".
    let range = match (rule.until, rule.count) {
        (Some(until), _) => ::ews::RecurrenceRange::EndDateRecurrence {
            start_date,
            end_date: chrono::DateTime::from_timestamp(until, 0)?
                .format("%Y-%m-%d")
                .to_string(),
        },
        (None, Some(count)) => ::ews::RecurrenceRange::NumberedRecurrence {
            start_date,
            number_of_occurrences: count.max(1),
        },
        (None, None) => ::ews::RecurrenceRange::NoEndRecurrence { start_date },
    };

    Some(::ews::Recurrence { pattern, range })
}

/// One of Exchange's recurrences as this codebase's own.
///
/// Only the patterns this client can put back on screen are read; anything
/// else — the relative monthly rules, for instance — is reported as no rule
/// rather than as a rule it would misrepresent, since showing "every month on
/// the 3rd" for "every third Tuesday" would be worse than showing nothing.
fn from_ews_recurrence(rule: &::ews::Recurrence) -> Option<crate::calendar::Recurrence> {
    use crate::calendar::{Frequency, Recurrence};
    let (freq, interval, weekdays) = match &rule.pattern {
        ::ews::RecurrencePattern::DailyRecurrence { interval } => {
            (Frequency::Daily, *interval, Vec::new())
        }
        ::ews::RecurrencePattern::WeeklyRecurrence {
            interval,
            days_of_week,
        } => (
            Frequency::Weekly,
            *interval,
            days_of_week
                .split_whitespace()
                .filter_map(|name| {
                    Recurrence::EWS_DAYS
                        .iter()
                        .position(|day| day.eq_ignore_ascii_case(name))
                        .map(|index| index as u8)
                })
                .collect(),
        ),
        ::ews::RecurrencePattern::AbsoluteMonthlyRecurrence { interval, .. } => {
            (Frequency::Monthly, *interval, Vec::new())
        }
        ::ews::RecurrencePattern::AbsoluteYearlyRecurrence { .. } => {
            (Frequency::Yearly, 1, Vec::new())
        }
    };

    let (until, count) = match &rule.range {
        ::ews::RecurrenceRange::NoEndRecurrence { .. } => (None, None),
        ::ews::RecurrenceRange::EndDateRecurrence { end_date, .. } => (parse_day(end_date), None),
        ::ews::RecurrenceRange::NumberedRecurrence {
            number_of_occurrences,
            ..
        } => (None, Some(*number_of_occurrences)),
    };

    Some(Recurrence {
        freq,
        interval: interval.max(1),
        weekdays,
        until,
        count,
    })
}

/// A `YYYY-MM-DD` day as the instant its last second falls on, which is what
/// "until this date, inclusive" means to the rest of this codebase.
fn parse_day(text: &str) -> Option<i64> {
    chrono::NaiveDate::parse_from_str(text.get(..10).unwrap_or(text), "%Y-%m-%d")
        .ok()?
        .and_hms_opt(23, 59, 0)
        .map(|naive| naive.and_utc().timestamp())
}

/// Builds the wire timestamp a write needs from epoch seconds.
fn ews_time(epoch_seconds: i64) -> ::ews::DateTime {
    ::ews::DateTime(
        time::OffsetDateTime::from_unix_timestamp(epoch_seconds)
            .unwrap_or(time::OffsetDateTime::UNIX_EPOCH),
    )
}

/// Reads one mailbox as a participant.
///
/// An on-premises server answers with an internal directory identifier rather
/// than an address for its own people — `/O=ORG/OU=.../CN=PCLAVE` — which is
/// unreadable and cannot be mailed to. The display name is what gets shown;
/// the identifier is kept out of `addr`, which is reserved for something that
/// really is an address. Turning one into the other needs `ResolveNames`,
/// which the crate does not implement yet, and is only actually required to
/// invite someone.
fn participant(mailbox: &::ews::Mailbox, response: Option<::ews::ResponseType>) -> Participant {
    let addr = mailbox
        .email_address
        .clone()
        .filter(|value| value.contains('@'))
        .unwrap_or_default();
    Participant {
        name: mailbox.name.clone().unwrap_or_default(),
        addr,
        response: response.map(response_name).unwrap_or_default(),
    }
}

fn response_name(response: ::ews::ResponseType) -> String {
    match response {
        ::ews::ResponseType::Accept => "accept",
        ::ews::ResponseType::Decline => "decline",
        ::ews::ResponseType::Tentative => "tentative",
        ::ews::ResponseType::Organizer => "organizer",
        ::ews::ResponseType::NoResponseReceived => "none",
        ::ews::ResponseType::Unknown => "",
    }
    .to_string()
}

fn free_busy_name(status: ::ews::LegacyFreeBusyStatus) -> String {
    match status {
        ::ews::LegacyFreeBusyStatus::Free => "free",
        ::ews::LegacyFreeBusyStatus::Tentative => "tentative",
        ::ews::LegacyFreeBusyStatus::Busy => "busy",
        ::ews::LegacyFreeBusyStatus::OOF => "oof",
        ::ews::LegacyFreeBusyStatus::WorkingElsewhere => "workingelsewhere",
        ::ews::LegacyFreeBusyStatus::NoData => "",
    }
    .to_string()
}

/// Projects one server occurrence onto the shared event model.
fn to_event(item: &Message, calendar_id: &str) -> anyhow::Result<Event> {
    let id = item
        .item_id
        .as_ref()
        .context("EWS returned a calendar item without an id")?;
    // An occurrence with no time is not an occurrence; refusing it is better
    // than drawing it at the epoch.
    let start = item
        .start
        .as_ref()
        .context("EWS returned a calendar item without a start")?;
    let end = item.end.as_ref().unwrap_or(start);
    Ok(Event {
        id: id.id.clone(),
        calendar_id: calendar_id.to_string(),
        change_key: id.change_key.clone(),
        subject: item.subject.clone().unwrap_or_default(),
        location: item.location.clone(),
        start: start.0.unix_timestamp(),
        end: end.0.unix_timestamp(),
        all_day: item.is_all_day_event.unwrap_or(false),
        is_recurring: item.is_recurring.unwrap_or(false),
            series_id: item.uid.clone(),
        recurrence: None,
        description: item
            .body
            .as_ref()
            .and_then(|body| body.content.as_deref())
            .map(crate::calendar::plain_notes)
            .unwrap_or_default(),
        // The minutes only mean something when a reminder is actually set:
        // Exchange keeps a value there either way.
        reminder_minutes: (item.reminder_is_set == Some(true))
            .then(|| item.reminder_minutes_before_start.map(|minutes| minutes as i64))
            .flatten(),
        is_cancelled: item.is_cancelled.unwrap_or(false),
        free_busy: item
            .legacy_free_busy_status
            .map(free_busy_name)
            .unwrap_or_default(),
        my_response: item.my_response_type.map(response_name).unwrap_or_default(),
        organizer: item
            .organizer
            .as_ref()
            .map(|who| participant(&who.mailbox, Some(::ews::ResponseType::Organizer))),
        attendees: item
            .required_attendees
            .iter()
            .chain(item.optional_attendees.iter())
            .flat_map(|list| list.0.iter())
            .map(|attendee| participant(&attendee.mailbox, attendee.response_type))
            .collect(),
    })
}

/// The properties an envelope fetch asks for, beyond the id the base shape
/// already carries.
///
/// `References` is what Meron's threading keys on; EWS exposes it as a typed
/// property, so there is no need to pull the whole internet header block for
/// the sake of one header.
///
/// `In-Reply-To` is requested as its MAPI property instead: the `ews` crate
/// carries the field on its message type, but `message:InReplyTo` is not in
/// the schema's requestable enumeration, and Exchange rejects the whole
/// request — with a bare `ErrorInvalidRequest` naming nothing — if it appears.
fn envelope_properties() -> Vec<PathToElement> {
    let mut properties: Vec<PathToElement> = [
        "item:Subject",
        "item:DateTimeSent",
        "message:From",
        "message:ToRecipients",
        "message:CcRecipients",
        "message:IsRead",
        "message:InternetMessageId",
        "message:References",
    ]
    .into_iter()
    .map(|uri| PathToElement::FieldURI {
        field_URI: uri.to_string(),
    })
    .collect();
    properties.push(in_reply_to_property());
    properties
}

/// `PidTagInReplyToId`, the MAPI property behind the `In-Reply-To` header.
fn in_reply_to_property() -> PathToElement {
    PathToElement::ExtendedFieldURI {
        distinguished_property_set_id: None,
        property_set_id: None,
        property_tag: Some("0x1042".to_string()),
        property_name: None,
        property_id: None,
        property_type: ::ews::PropertyType::String,
    }
}

/// Reads the `In-Reply-To` value back out of the extended properties.
fn in_reply_to(message: &Message) -> Option<String> {
    message
        .extended_property
        .as_ref()?
        .iter()
        .find(|property| {
            matches!(
                &property.extended_field_URI,
                ::ews::ExtendedFieldURI { property_tag: Some(tag), .. }
                    if tag.eq_ignore_ascii_case("0x1042")
            )
        })
        .map(|property| crate::imap::normalize_message_id(&property.value))
}

/// Envelope fields for one message, as the sync path needs them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EwsEnvelope {
    pub item_id: String,
    pub change_key: Option<String>,
    pub subject: String,
    pub from_name: String,
    pub from_addr: String,
    /// Send time as Unix epoch seconds, 0 when the server omitted it.
    pub date: i64,
    pub seen: bool,
    pub message_id: String,
    pub in_reply_to: String,
    /// First id of the `References` header, the root of the thread.
    pub references_root: String,
    pub to: Vec<crate::imap::Recipient>,
    pub cc: Vec<crate::imap::Recipient>,
}

impl EwsEnvelope {
    fn from_message(message: &Message) -> anyhow::Result<Self> {
        let id = message
            .item_id
            .as_ref()
            .context("EWS returned a message without an item id")?;
        Ok(Self {
            item_id: id.id.clone(),
            change_key: id.change_key.clone(),
            subject: message.subject.clone().unwrap_or_default(),
            from_name: mailbox_name(message.from.as_ref()),
            from_addr: mailbox_addr(message.from.as_ref()),
            date: message
                .date_time_sent
                .as_ref()
                .map(|sent| sent.0.unix_timestamp())
                .unwrap_or_default(),
            // Absent IsRead means the server did not return the property, not
            // that the message is unread; only a present `false` is unread.
            seen: message.is_read.unwrap_or(true),
            message_id: message
                .internet_message_id
                .as_deref()
                .map(crate::imap::normalize_message_id)
                .unwrap_or_default(),
            // The typed field is never populated: the property is requested
            // through its MAPI tag, so the value arrives in extended_property.
            in_reply_to: in_reply_to(message).unwrap_or_default(),
            references_root: references_root(message).unwrap_or_default(),
            to: recipients(message.to_recipients.as_ref()),
            cc: recipients(message.cc_recipients.as_ref()),
        })
    }

    /// Projects the envelope onto the header type the rest of the core stores
    /// and renders, under the local uid its item maps to.
    pub fn into_header(self, uid: u32, folder: &str) -> crate::imap::MessageHeader {
        let thread_key = crate::imap::thread_key(
            // X-GM-THRID is Gmail-only; Exchange threads by ConversationId,
            // which is not the same grouping and is not consulted here.
            None,
            &self.message_id,
            &self.in_reply_to,
            &self.references_root,
            uid,
        );
        crate::imap::MessageHeader {
            uid,
            folder: folder.to_string(),
            subject: self.subject,
            from_name: self.from_name,
            from_addr: self.from_addr,
            date: self.date,
            seen: self.seen,
            // Exchange represents follow-up flags as a MAPI property rather
            // than an IMAP-style keyword; until the sync path requests it,
            // report unflagged rather than guessing.
            starred: false,
            thread_key,
            message_id: self.message_id,
            gmail_msg_id: None,
            in_reply_to: self.in_reply_to,
            to: self.to,
            cc: self.cc,
            recipient_overflow: 0,
        }
    }
}

/// The thread root: the first id of `References`, which is the oldest
/// ancestor the message names.
fn references_root(message: &Message) -> Option<String> {
    message
        .references
        .as_deref()
        .and_then(crate::imap::first_message_id)
}

fn mailbox_name(recipient: Option<&::ews::Recipient>) -> String {
    recipient
        .and_then(|recipient| recipient.mailbox.name.clone())
        .unwrap_or_default()
}

fn mailbox_addr(recipient: Option<&::ews::Recipient>) -> String {
    recipient
        .and_then(|recipient| recipient.mailbox.email_address.clone())
        .unwrap_or_default()
}

fn recipients(list: Option<&::ews::ArrayOfRecipients>) -> Vec<crate::imap::Recipient> {
    list.map(|list| {
        list.0
            .iter()
            .map(|recipient| crate::imap::Recipient {
                name: recipient.mailbox.name.clone().unwrap_or_default(),
                addr: recipient.mailbox.email_address.clone().unwrap_or_default(),
            })
            .collect()
    })
    .unwrap_or_default()
}

fn folder_id(reference: &EwsId) -> BaseFolderId {
    BaseFolderId::FolderId {
        id: reference.0.clone(),
        change_key: reference.1.clone(),
    }
}

fn item_id(reference: &EwsId) -> BaseItemId {
    BaseItemId::ItemId {
        id: reference.0.clone(),
        change_key: reference.1.clone(),
    }
}

/// The item a write should address: the occurrence itself, or the series it
/// belongs to.
///
/// An occurrence id cannot reach its master any other way, which is why the
/// series case is a different kind of identifier rather than a different id.
fn scoped_item_id(reference: &EwsId, whole_series: bool) -> BaseItemId {
    if whole_series {
        BaseItemId::RecurringMasterItemId {
            occurrence_id: reference.0.clone(),
            change_key: reference.1.clone(),
        }
    } else {
        item_id(reference)
    }
}

fn basic_auth(username: &str, password: &str) -> String {
    let credentials =
        base64::engine::general_purpose::STANDARD.encode(format!("{username}:{password}"));
    format!("Basic {credentials}")
}

/// Whether a folder sits in the wastebasket, and so is a calendar the user has
/// already removed.
///
/// Removing a calendar moves it to Deleted Items rather than destroying it, so
/// the folder outlives the removal and the hierarchy keeps reporting it.
/// Listing it again would quietly undo the removal that was asked for.
fn is_discarded(deleted_items: Option<&str>, parent: Option<&::ews::FolderId>) -> bool {
    match (deleted_items, parent) {
        (Some(bin), Some(parent)) => parent.id == bin,
        // With no wastebasket to compare against, nothing is known to be in it:
        // hiding a calendar the user still has would be the worse mistake.
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ::ews::sync_folder_hierarchy::Change;

    fn folder(id: &str) -> ::ews::FolderId {
        ::ews::FolderId {
            id: id.to_string(),
            change_key: None,
        }
    }

    #[test]
    fn an_answer_names_the_request_it_answers_and_which_answer_it_is() {
        use crate::calendar::Response;
        let reference = |response: Response| {
            let item = Message {
                reference_item_id: Some(::ews::ReferenceItemId {
                    id: "AAMkRequest=".to_string(),
                    change_key: Some("CK9".to_string()),
                }),
                ..Message::default()
            };
            let answer = match response {
                Response::Accept => RealItem::AcceptItem(item),
                Response::Tentative => RealItem::TentativelyAcceptItem(item),
                Response::Decline => RealItem::DeclineItem(item),
            };
            let request = build_request(CreateItem {
                message_disposition: Some(MessageDisposition::SendAndSaveCopy),
                send_meeting_invitations: None,
                saved_item_folder_id: None,
                items: vec![answer],
            })
            .expect("serialization should succeed");
            String::from_utf8(request).expect("UTF-8")
        };

        let accepted = reference(Response::Accept);
        assert!(accepted.contains("AcceptItem"), "{accepted}");
        assert!(accepted.contains("ReferenceItemId"), "the request answered: {accepted}");
        assert!(accepted.contains("AAMkRequest="), "{accepted}");
        // Sent, not merely filed: an answer nobody receives is not an answer.
        assert!(accepted.contains("SendAndSaveCopy"), "{accepted}");

        // The three must not be confused for one another — telling an
        // organizer the opposite of what was meant is the worst failure here.
        let declined = reference(Response::Decline);
        assert!(declined.contains("DeclineItem"), "{declined}");
        assert!(!declined.contains("AcceptItem"), "{declined}");
        assert!(reference(Response::Tentative).contains("TentativelyAcceptItem"));
    }

    #[test]
    fn a_write_addresses_the_occurrence_or_its_master_but_never_both() {
        let reference = ("AAMkOccurrence=".to_string(), Some("CK1".to_string()));

        // This day only: the occurrence's own id.
        match scoped_item_id(&reference, false) {
            BaseItemId::ItemId { id, change_key } => {
                assert_eq!(id, "AAMkOccurrence=");
                assert_eq!(change_key.as_deref(), Some("CK1"));
            }
            other => panic!("an occurrence must be addressed by its own id: {other:?}"),
        }

        // The whole series: the master, reached through that same occurrence.
        // Getting this wrong would change appointments the reader never opened.
        match scoped_item_id(&reference, true) {
            BaseItemId::RecurringMasterItemId {
                occurrence_id,
                change_key,
            } => {
                assert_eq!(occurrence_id, "AAMkOccurrence=");
                assert_eq!(change_key.as_deref(), Some("CK1"));
            }
            other => panic!("a series must be addressed through its master: {other:?}"),
        }
    }

    #[test]
    fn a_weekly_series_serializes_the_way_exchange_expects() {
        use crate::calendar::{Frequency, Recurrence};
        // Thursday 27 August 2026, 12:00 UTC.
        let start = chrono::DateTime::parse_from_rfc3339("2026-08-27T12:00:00Z")
            .unwrap()
            .timestamp();
        let event = Event {
            subject: "Reunió".to_string(),
            start,
            end: start + 3600,
            recurrence: Some(Recurrence {
                freq: Frequency::Weekly,
                interval: 2,
                weekdays: vec![1, 3],
                until: Some(start + 60 * 24 * 3600),
                count: None,
                }),
            ..Default::default()
        };
        let request = build_request(CreateItem {
            message_disposition: None,
            send_meeting_invitations: Some(::ews::create_item::SendMeetingInvitations::SendToNone),
            saved_item_folder_id: None,
            items: vec![RealItem::CalendarItem(event_item(&event))],
        })
        .expect("serialization should succeed");
        let xml = String::from_utf8(request).expect("UTF-8");

        assert!(xml.contains("WeeklyRecurrence"), "missing pattern: {xml}");
        assert!(xml.contains("Tuesday Thursday"), "days must be named, space separated: {xml}");
        assert!(xml.contains("EndDateRecurrence"), "missing range: {xml}");
        assert!(xml.contains("2026-08-27"), "the range starts on the event's own day: {xml}");

        // The pattern must precede the range: the server rejects the whole
        // request if they arrive the other way round.
        let pattern_at = xml.find("WeeklyRecurrence").unwrap();
        let range_at = xml.find("EndDateRecurrence").unwrap();
        assert!(pattern_at < range_at, "pattern must come before range: {xml}");
    }

    #[test]
    fn an_event_without_a_rule_carries_no_recurrence() {
        let event = Event {
            subject: "Un cop".to_string(),
            start: 1_788_249_600,
            end: 1_788_253_200,
            ..Default::default()
        };
        let request = build_request(CreateItem {
            message_disposition: None,
            send_meeting_invitations: Some(::ews::create_item::SendMeetingInvitations::SendToNone),
            saved_item_folder_id: None,
            items: vec![RealItem::CalendarItem(event_item(&event))],
        })
        .expect("serialization should succeed");
        let xml = String::from_utf8(request).expect("UTF-8");
        assert!(!xml.contains("Recurrence"), "a one-off must not claim a rule: {xml}");
    }

    #[test]
    fn a_calendar_in_the_wastebasket_is_not_listed_again() {
        let bin = "deleted-items";
        // The case that matters: removal moves the folder here, and listing it
        // again would undo the removal the user asked for.
        assert!(is_discarded(Some(bin), Some(&folder(bin))));
        assert!(!is_discarded(Some(bin), Some(&folder("inbox"))));

        // Unknown parentage must not hide a calendar the user still has.
        assert!(!is_discarded(Some(bin), None));
        assert!(!is_discarded(None, Some(&folder(bin))));
    }

    #[test]
    fn basic_auth_encodes_rfc7617_credentials() {
        // Canonical example pair, plus a DOMAIN\user form.
        assert_eq!(basic_auth("Aladdin", "open sesame"), "Basic QWxhZGRpbjpvcGVuIHNlc2FtZQ==");
        assert_eq!(basic_auth("CORP\\user", "pw"), "Basic Q09SUFx1c2VyOnB3");
    }

    #[test]
    fn build_request_produces_a_complete_soap_document() {
        let request = build_request(SyncFolderHierarchy {
            folder_shape: FolderShape {
                base_shape: BaseShape::IdOnly,
            },
            sync_folder_id: Some(BaseFolderId::DistinguishedFolderId {
                id: "msgfolderroot".to_string(),
                change_key: None,
                mailbox: None,
            }),
            sync_state: None,
        })
        .expect("serialization should succeed");
        let xml = String::from_utf8(request).expect("request should be UTF-8");
        assert!(xml.contains("soap:Envelope"), "missing SOAP envelope: {xml}");
        assert!(xml.contains("RequestServerVersion"), "missing version header: {xml}");
        assert!(xml.contains("SyncFolderHierarchy"), "missing operation body: {xml}");
        assert!(xml.contains("msgfolderroot"), "missing folder id: {xml}");
    }

    /// Response shaped like a real server's first hierarchy sync round.
    const HIERARCHY_RESPONSE: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<s:Envelope xmlns:s="http://schemas.xmlsoap.org/soap/envelope/">
  <s:Header>
    <h:ServerVersionInfo MajorVersion="15" MinorVersion="2" MajorBuildNumber="2562" MinorBuildNumber="43" Version="V2017_07_11" xmlns:h="http://schemas.microsoft.com/exchange/services/2006/types"/>
  </s:Header>
  <s:Body>
    <m:SyncFolderHierarchyResponse xmlns:m="http://schemas.microsoft.com/exchange/services/2006/messages" xmlns:t="http://schemas.microsoft.com/exchange/services/2006/types">
      <m:ResponseMessages>
        <m:SyncFolderHierarchyResponseMessage ResponseClass="Success">
          <m:ResponseCode>NoError</m:ResponseCode>
          <m:SyncState>H4sIAAAAAAAEAO29B2AcSZY=</m:SyncState>
          <m:IncludesLastFolderInRange>true</m:IncludesLastFolderInRange>
          <m:Changes>
            <t:Create>
              <t:Folder>
                <t:FolderId Id="AAMkAGZmZTk=" ChangeKey="AQAAABYAAAA="/>
                <t:DisplayName>Inbox</t:DisplayName>
                <t:TotalCount>228</t:TotalCount>
                <t:ChildFolderCount>0</t:ChildFolderCount>
                <t:UnreadCount>5</t:UnreadCount>
              </t:Folder>
            </t:Create>
          </m:Changes>
        </m:SyncFolderHierarchyResponseMessage>
      </m:ResponseMessages>
    </m:SyncFolderHierarchyResponse>
  </s:Body>
</s:Envelope>"#;

    /// Response shaped like an item-sync round carrying one new message, one
    /// read-flag change, and one deletion.
    const ITEM_SYNC_RESPONSE: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<s:Envelope xmlns:s="http://schemas.xmlsoap.org/soap/envelope/">
  <s:Header/>
  <s:Body>
    <m:SyncFolderItemsResponse xmlns:m="http://schemas.microsoft.com/exchange/services/2006/messages" xmlns:t="http://schemas.microsoft.com/exchange/services/2006/types">
      <m:ResponseMessages>
        <m:SyncFolderItemsResponseMessage ResponseClass="Success">
          <m:ResponseCode>NoError</m:ResponseCode>
          <m:SyncState>c3RhdGUy</m:SyncState>
          <m:IncludesLastItemInRange>false</m:IncludesLastItemInRange>
          <m:Changes>
            <t:Create>
              <t:Message>
                <t:ItemId Id="AAMkNEW=" ChangeKey="CQAAAA=="/>
              </t:Message>
            </t:Create>
            <t:ReadFlagChange>
              <t:ItemId Id="AAMkREAD=" ChangeKey="CQAAAB=="/>
              <t:IsRead>true</t:IsRead>
            </t:ReadFlagChange>
            <t:Delete>
              <t:ItemId Id="AAMkGONE=" ChangeKey="CQAAAC=="/>
            </t:Delete>
          </m:Changes>
        </m:SyncFolderItemsResponseMessage>
      </m:ResponseMessages>
    </m:SyncFolderItemsResponse>
  </s:Body>
</s:Envelope>"#;

    #[test]
    fn parse_response_reads_an_item_sync_round() {
        use ::ews::sync_folder_items::Change as ItemChange;
        let response = parse_response::<SyncFolderItems>(ITEM_SYNC_RESPONSE.as_bytes())
            .expect("parse should succeed");
        let message = single_message(into_successes(response).expect("success"))
            .expect("one response message");
        assert_eq!(message.sync_state, "c3RhdGUy");
        assert!(!message.includes_last_item_in_range);
        let changes = &message.changes.inner;
        assert_eq!(changes.len(), 3);
        assert!(matches!(&changes[0], ItemChange::Create { .. }));
        match &changes[1] {
            ItemChange::ReadFlagChange { item_id, is_read } => {
                assert_eq!(item_id.id, "AAMkREAD=");
                assert!(is_read);
            }
            other => panic!("expected ReadFlagChange, got {other:?}"),
        }
        match &changes[2] {
            ItemChange::Delete { item_id } => assert_eq!(item_id.id, "AAMkGONE="),
            other => panic!("expected Delete, got {other:?}"),
        }
    }

    /// GetItem response carrying base64 MIME content ("Subject: hi\r\n\r\nbody").
    const GET_ITEM_RESPONSE: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<s:Envelope xmlns:s="http://schemas.xmlsoap.org/soap/envelope/">
  <s:Header/>
  <s:Body>
    <m:GetItemResponse xmlns:m="http://schemas.microsoft.com/exchange/services/2006/messages" xmlns:t="http://schemas.microsoft.com/exchange/services/2006/types">
      <m:ResponseMessages>
        <m:GetItemResponseMessage ResponseClass="Success">
          <m:ResponseCode>NoError</m:ResponseCode>
          <m:Items>
            <t:Message>
              <t:MimeContent CharacterSet="UTF-8">U3ViamVjdDogaGkNCg0KYm9keQ==</t:MimeContent>
              <t:ItemId Id="AAMkMIME=" ChangeKey="CQAAAD=="/>
            </t:Message>
          </m:Items>
        </m:GetItemResponseMessage>
      </m:ResponseMessages>
    </m:GetItemResponse>
  </s:Body>
</s:Envelope>"#;

    #[test]
    fn build_request_for_send_encodes_mime_and_disposition() {
        let request = build_request(CreateItem {
            message_disposition: Some(MessageDisposition::SendAndSaveCopy),
            send_meeting_invitations: None,
            saved_item_folder_id: None,
            items: vec![RealItem::Message(Message {
                mime_content: Some(MimeContent {
                    character_set: None,
                    content: base64::engine::general_purpose::STANDARD.encode(b"Subject: hi\r\n\r\nbody"),
                }),
                ..Message::default()
            })],
        })
        .expect("serialization should succeed");
        let xml = String::from_utf8(request).expect("UTF-8");
        assert!(xml.contains("SendAndSaveCopy"), "missing disposition: {xml}");
        assert!(xml.contains("U3ViamVjdDogaGkNCg0KYm9keQ=="), "missing MIME payload: {xml}");
    }

    #[test]
    fn build_request_for_read_flag_targets_the_field_uri() {
        let request = build_request(UpdateItem {
            message_disposition: MessageDisposition::SaveOnly,
            send_meeting_invitations_or_cancellations: None,
            conflict_resolution: Some(ConflictResolution::AutoResolve),
            item_changes: vec![ItemChange {
                item_change: ItemChangeInner {
                    item_id: item_id(&("AAMk=".to_string(), Some("CQAA".to_string()))),
                    updates: Updates {
                        inner: vec![ItemChangeDescription::SetItemField {
                            field_uri: PathToElement::FieldURI {
                                field_URI: "message:IsRead".to_string(),
                            },
                            item: RealItem::Message(Message {
                                is_read: Some(true),
                                ..Message::default()
                            }),
                        }],
                    },
                },
            }],
        })
        .expect("serialization should succeed");
        let xml = String::from_utf8(request).expect("UTF-8");
        assert!(xml.contains("message:IsRead"), "missing field URI: {xml}");
        assert!(xml.contains("IsRead"), "missing flag value: {xml}");
        assert!(xml.contains("AAMk="), "missing item id: {xml}");
    }

    /// A reply carrying every envelope property the sync path requests.
    const ENVELOPE_RESPONSE: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<s:Envelope xmlns:s="http://schemas.xmlsoap.org/soap/envelope/">
  <s:Header/>
  <s:Body>
    <m:GetItemResponse xmlns:m="http://schemas.microsoft.com/exchange/services/2006/messages" xmlns:t="http://schemas.microsoft.com/exchange/services/2006/types">
      <m:ResponseMessages>
        <m:GetItemResponseMessage ResponseClass="Success">
          <m:ResponseCode>NoError</m:ResponseCode>
          <m:Items>
            <t:Message>
              <t:ItemId Id="AAMkENV=" ChangeKey="CQAAAE=="/>
              <t:Subject>RE: Quarterly report</t:Subject>
              <t:DateTimeSent>2026-08-24T09:15:00Z</t:DateTimeSent>
              <t:References>&lt;root@example.org&gt; &lt;reply@example.org&gt;</t:References>
              <t:From>
                <t:Mailbox>
                  <t:Name>Ada Lovelace</t:Name>
                  <t:EmailAddress>ada@example.org</t:EmailAddress>
                </t:Mailbox>
              </t:From>
              <t:ToRecipients>
                <t:Mailbox>
                  <t:Name>Team</t:Name>
                  <t:EmailAddress>team@example.org</t:EmailAddress>
                </t:Mailbox>
              </t:ToRecipients>
              <t:CcRecipients>
                <t:Mailbox>
                  <t:EmailAddress>watcher@example.org</t:EmailAddress>
                </t:Mailbox>
              </t:CcRecipients>
              <t:InternetMessageId>&lt;current@example.org&gt;</t:InternetMessageId>
              <t:ExtendedProperty>
                <t:ExtendedFieldURI PropertyTag="0x1042" PropertyType="String"/>
                <t:Value>&lt;reply@example.org&gt;</t:Value>
              </t:ExtendedProperty>
              <t:IsRead>false</t:IsRead>
            </t:Message>
          </m:Items>
        </m:GetItemResponseMessage>
      </m:ResponseMessages>
    </m:GetItemResponse>
  </s:Body>
</s:Envelope>"#;

    #[test]
    fn envelope_fetch_maps_every_field_without_downloading_bodies() {
        let (url, server) = serve_soap_once(ENVELOPE_RESPONSE);
        let client = EwsClient::new(EwsConfig {
            url,
            username: "u".to_string(),
            password: "p".to_string(),
        });
        let envelopes = client
            .fetch_envelopes(&[("AAMkENV=".to_string(), None)])
            .expect("envelope fetch should succeed");

        assert_eq!(envelopes.len(), 1);
        let envelope = &envelopes[0];
        assert_eq!(envelope.item_id, "AAMkENV=");
        assert_eq!(envelope.change_key.as_deref(), Some("CQAAAE=="));
        assert_eq!(envelope.subject, "RE: Quarterly report");
        assert_eq!(envelope.from_name, "Ada Lovelace");
        assert_eq!(envelope.from_addr, "ada@example.org");
        // 2026-08-24T09:15:00Z as epoch seconds.
        assert_eq!(envelope.date, 1787562900);
        assert!(!envelope.seen, "IsRead=false must map to unseen");
        // Angle brackets are stripped, casing preserved.
        assert_eq!(envelope.message_id, "current@example.org");
        assert_eq!(envelope.in_reply_to, "reply@example.org");
        // The thread root is the FIRST id of References, not the last.
        assert_eq!(envelope.references_root, "root@example.org");
        assert_eq!(
            envelope.to,
            vec![crate::imap::Recipient {
                name: "Team".to_string(),
                addr: "team@example.org".to_string(),
            }]
        );
        assert_eq!(
            envelope.cc,
            vec![crate::imap::Recipient {
                name: String::new(),
                addr: "watcher@example.org".to_string(),
            }]
        );

        let request = server.join().expect("server thread");
        assert!(
            !request.contains("IncludeMimeContent"),
            "envelope fetch must not download bodies: {request}"
        );
        for property in ["item:Subject", "message:From", "message:IsRead"] {
            assert!(request.contains(property), "missing {property}: {request}");
        }
        // Exchange rejects the whole request if `message:InReplyTo` appears,
        // so it must be requested through its MAPI tag instead.
        assert!(
            !request.contains("message:InReplyTo"),
            "InReplyTo must not be requested as a field URI: {request}"
        );
        assert!(request.contains("0x1042"), "missing the In-Reply-To MAPI tag: {request}");
    }

    #[test]
    fn envelope_threads_on_the_references_root() {
        let envelope = EwsEnvelope {
            item_id: "AAMk=".to_string(),
            change_key: None,
            subject: "RE: hi".to_string(),
            from_name: "A".to_string(),
            from_addr: "a@example.org".to_string(),
            date: 1787562900,
            seen: false,
            message_id: "current@example.org".to_string(),
            in_reply_to: "parent@example.org".to_string(),
            references_root: "root@example.org".to_string(),
            to: Vec::new(),
            cc: Vec::new(),
        };

        let header = envelope.clone().into_header(7, "INBOX");
        assert_eq!(header.uid, 7);
        assert_eq!(header.folder, "INBOX");
        // References root wins over In-Reply-To and the message's own id, so a
        // reply lands in the same thread as its ancestors.
        assert_eq!(header.thread_key, "root@example.org");
        assert!(!header.starred, "Exchange follow-up flags are not read yet");
        assert_eq!(header.gmail_msg_id, None);

        // With no References the parent id carries the thread instead.
        let orphan = EwsEnvelope {
            references_root: String::new(),
            ..envelope
        };
        assert_eq!(orphan.into_header(7, "INBOX").thread_key, "parent@example.org");
    }

    #[test]
    fn envelope_defaults_do_not_invent_unread_mail() {
        // A server that omits IsRead must not make every message look unread:
        // an absent property means "not returned", not "unread".
        let message = ::ews::Message {
            item_id: Some(::ews::ItemId {
                id: "AAMk=".to_string(),
                change_key: None,
            }),
            ..::ews::Message::default()
        };
        let envelope = EwsEnvelope::from_message(&message).expect("mapping should succeed");
        assert!(envelope.seen);
        assert_eq!(envelope.date, 0);
        assert_eq!(envelope.subject, "");

        // Without any id at all the message is unusable and must fail loud.
        let anonymous = ::ews::Message::default();
        assert!(EwsEnvelope::from_message(&anonymous).is_err());
    }

    /// Hierarchy reply mixing a mail folder with the calendar and contacts
    /// folders every Exchange mailbox also carries.
    const MIXED_HIERARCHY_RESPONSE: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<s:Envelope xmlns:s="http://schemas.xmlsoap.org/soap/envelope/">
  <s:Header/>
  <s:Body>
    <m:SyncFolderHierarchyResponse xmlns:m="http://schemas.microsoft.com/exchange/services/2006/messages" xmlns:t="http://schemas.microsoft.com/exchange/services/2006/types">
      <m:ResponseMessages>
        <m:SyncFolderHierarchyResponseMessage ResponseClass="Success">
          <m:ResponseCode>NoError</m:ResponseCode>
          <m:SyncState>aGllcg==</m:SyncState>
          <m:IncludesLastFolderInRange>true</m:IncludesLastFolderInRange>
          <m:Changes>
            <t:Create>
              <t:Folder>
                <t:FolderId Id="AAMkINBOX=" ChangeKey="CK-INBOX"/>
                <t:FolderClass>IPF.Note</t:FolderClass>
                <t:DisplayName>Inbox</t:DisplayName>
                <t:UnreadCount>5</t:UnreadCount>
              </t:Folder>
            </t:Create>
            <t:Create>
              <t:Folder>
                <t:FolderId Id="AAMkDIARIO=" ChangeKey="CK-D"/>
                <t:FolderClass>IPF.Journal</t:FolderClass>
                <t:DisplayName>Diario</t:DisplayName>
              </t:Folder>
            </t:Create>
            <t:Create>
              <t:Folder>
                <t:FolderId Id="AAMkCFG=" ChangeKey="CK-CFG"/>
                <t:FolderClass>IPF.Configuration</t:FolderClass>
                <t:DisplayName>Conversation Action Settings</t:DisplayName>
              </t:Folder>
            </t:Create>
            <t:Create>
              <t:CalendarFolder>
                <t:FolderId Id="AAMkCAL=" ChangeKey="CK-CAL"/>
                <t:DisplayName>Calendar</t:DisplayName>
              </t:CalendarFolder>
            </t:Create>
            <t:Create>
              <t:ContactsFolder>
                <t:FolderId Id="AAMkCON=" ChangeKey="CK-CON"/>
                <t:DisplayName>Contacts</t:DisplayName>
              </t:ContactsFolder>
            </t:Create>
            <t:Create>
              <t:Folder>
                <t:FolderId Id="AAMkSENT=" ChangeKey="CK-SENT"/>
                <t:DisplayName>Sent Items</t:DisplayName>
              </t:Folder>
            </t:Create>
          </m:Changes>
        </m:SyncFolderHierarchyResponseMessage>
      </m:ResponseMessages>
    </m:SyncFolderHierarchyResponse>
  </s:Body>
</s:Envelope>"#;

    type TestDb = std::sync::Arc<std::sync::Mutex<rusqlite::Connection>>;

    fn test_db() -> TestDb {
        std::sync::Arc::new(std::sync::Mutex::new(
            crate::store::open_in_memory_for_test().expect("open store"),
        ))
    }

    fn test_session(url: String) -> (EwsSession, TestDb) {
        let db = test_db();
        let session = EwsSession::new(
            EwsConfig {
                url,
                username: "u".to_string(),
                password: "p".to_string(),
            },
            "acct",
            db.clone(),
        );
        (session, db)
    }

    #[tokio::test]
    async fn listing_folders_keeps_mail_and_drops_calendar_and_contacts() {
        let (url, server) = serve_soap(vec![
            MIXED_HIERARCHY_RESPONSE.to_string(),
            distinguished_response(),
        ]);
        let (mut session, _db) = test_session(url);

        let folders = session.list_folders().await.expect("list should succeed");
        let names: Vec<&str> = folders.iter().map(|f| f.name.as_str()).collect();
        // Calendar and contacts are distinct folder types; the journal and the
        // settings folder share the mail type and are told apart by their
        // container class — the fixture names them in Spanish precisely
        // because a name-based rule would miss them on a localised mailbox.
        // The inbox is addressed as INBOX, which every default in the core
        // assumes, while its own name stays on display.
        assert_eq!(
            names,
            vec!["INBOX", "Sent Items"],
            "only folders that hold mail should be listed"
        );
        assert_eq!(folders[0].display_name, "Inbox");
        assert_eq!(folders[0].unread, 5);
        // Roles come from the distinguished ids, not from folder names, so
        // Sent is found on a mailbox in any language.
        assert_eq!(folders[0].special_use.as_deref(), Some("inbox"));
        assert_eq!(folders[1].special_use.as_deref(), Some("sent"));

        server.join().expect("server thread");
    }

    #[test]
    fn unlabelled_folders_are_kept_and_housekeeping_ones_dropped() {
        assert!(holds_mail(Some(&"IPF.Note".to_string())));
        // Hiding real mail is worse than showing one stray folder.
        assert!(holds_mail(None));
        for class in ["IPF.Journal", "IPF.Configuration", "IPF.Contact", "IPF.Appointment"] {
            assert!(!holds_mail(Some(&class.to_string())), "{class} is not mail");
        }
    }

    #[tokio::test]
    async fn syncing_a_folder_numbers_every_message_in_arrival_order() {
        let (url, server) = serve_soap_once(THREE_ITEM_SYNC_RESPONSE);
        let (mut session, db) = test_session(url);
        session
            .folders
            .insert("INBOX".to_string(), ("AAMkINBOX=".to_string(), None));

        // Only the newest message's envelope is fetched, but all three items
        // must be numbered, oldest first.
        let _ = session.fetch_recent("INBOX", 1).await;

        let conn = db.lock().unwrap();
        let uid_of = |id: &str| {
            crate::store::map_ews_item(&conn, "acct", "INBOX", id, None).expect("mapped")
        };
        assert_eq!(uid_of("AAMkOLD="), 1, "the oldest message keeps the lowest uid");
        assert_eq!(uid_of("AAMkMID="), 2);
        assert_eq!(uid_of("AAMkNEW="), 3, "the newest message gets the highest uid");

        server.join().expect("server thread");
    }

    /// An item-sync round listing three messages, oldest first.
    const THREE_ITEM_SYNC_RESPONSE: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<s:Envelope xmlns:s="http://schemas.xmlsoap.org/soap/envelope/">
  <s:Header/>
  <s:Body>
    <m:SyncFolderItemsResponse xmlns:m="http://schemas.microsoft.com/exchange/services/2006/messages" xmlns:t="http://schemas.microsoft.com/exchange/services/2006/types">
      <m:ResponseMessages>
        <m:SyncFolderItemsResponseMessage ResponseClass="Success">
          <m:ResponseCode>NoError</m:ResponseCode>
          <m:SyncState>c3RhdGU=</m:SyncState>
          <m:IncludesLastItemInRange>true</m:IncludesLastItemInRange>
          <m:Changes>
            <t:Create><t:Message><t:ItemId Id="AAMkOLD=" ChangeKey="CK1"/></t:Message></t:Create>
            <t:Create><t:Message><t:ItemId Id="AAMkMID=" ChangeKey="CK2"/></t:Message></t:Create>
            <t:Create><t:Message><t:ItemId Id="AAMkNEW=" ChangeKey="CK3"/></t:Message></t:Create>
          </m:Changes>
        </m:SyncFolderItemsResponseMessage>
      </m:ResponseMessages>
    </m:SyncFolderItemsResponse>
  </s:Body>
</s:Envelope>"#;

    /// One occurrence as an on-premises server really answers: an internal
    /// organizer identified by a directory name rather than an address.
    const CALENDAR_VIEW_RESPONSE: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<s:Envelope xmlns:s="http://schemas.xmlsoap.org/soap/envelope/"><s:Header/><s:Body>
  <m:FindItemResponse xmlns:m="http://schemas.microsoft.com/exchange/services/2006/messages" xmlns:t="http://schemas.microsoft.com/exchange/services/2006/types">
    <m:ResponseMessages><m:FindItemResponseMessage ResponseClass="Success">
      <m:ResponseCode>NoError</m:ResponseCode>
      <m:RootFolder TotalItemsInView="1" IncludesLastItemInRange="true">
        <t:Items>
          <t:CalendarItem>
            <t:ItemId Id="AAMkEV=" ChangeKey="CK-EV"/>
            <t:Subject>Reunió equip</t:Subject>
            <t:Start>2026-09-03T09:00:00Z</t:Start>
            <t:End>2026-09-03T11:00:00Z</t:End>
            <t:IsAllDayEvent>false</t:IsAllDayEvent>
            <t:LegacyFreeBusyStatus>Busy</t:LegacyFreeBusyStatus>
            <t:Location>Sala oval</t:Location>
            <t:IsRecurring>true</t:IsRecurring>
            <t:Organizer>
              <t:Mailbox>
                <t:Name>Pere Clavé</t:Name>
                <t:EmailAddress>/O=CONSORCI/OU=GRUPO/CN=RECIPIENTS/CN=PCLAVE</t:EmailAddress>
                <t:RoutingType>EX</t:RoutingType>
              </t:Mailbox>
            </t:Organizer>
            <t:RequiredAttendees>
              <t:Attendee>
                <t:Mailbox>
                  <t:Name>Adil</t:Name>
                  <t:EmailAddress>adil@example.org</t:EmailAddress>
                </t:Mailbox>
                <t:ResponseType>Accept</t:ResponseType>
              </t:Attendee>
            </t:RequiredAttendees>
            <t:MyResponseType>Accept</t:MyResponseType>
          </t:CalendarItem>
        </t:Items>
      </m:RootFolder>
    </m:FindItemResponseMessage></m:ResponseMessages>
  </m:FindItemResponse></s:Body></s:Envelope>"#;

    #[tokio::test]
    async fn a_calendar_window_maps_occurrences_and_keeps_names_readable() {
        let (url, server) = serve_soap_once(CALENDAR_VIEW_RESPONSE);
        let (mut session, _db) = test_session(url);
        session
            .calendars
            .insert("cal".to_string(), ("AAMkCAL=".to_string(), None));

        let events = session
            .events_in_window("cal", 1787_000_000, 1790_000_000)
            .await
            .expect("window should map");

        assert_eq!(events.len(), 1);
        let event = &events[0];
        assert_eq!(event.subject, "Reunió equip");
        assert_eq!(event.location.as_deref(), Some("Sala oval"));
        // 2026-09-03 09:00–11:00 UTC.
        assert_eq!(event.start, 1788426000);
        assert_eq!(event.end, 1788433200);
        assert!(event.is_recurring, "an expanded occurrence knows it is one");
        assert_eq!(event.free_busy, "busy");
        assert_eq!(event.my_response, "accept");

        // The organizer is internal, so the server answers with a directory
        // identifier. Showing that would be unreadable and mailing it would
        // fail, so the name carries the person and `addr` stays empty until
        // something can resolve it.
        let organizer = event.organizer.as_ref().expect("organizer");
        assert_eq!(organizer.name, "Pere Clavé");
        assert_eq!(organizer.addr, "", "a directory identifier is not an address");

        // A real address passes through untouched.
        assert_eq!(event.attendees.len(), 1);
        assert_eq!(event.attendees[0].addr, "adil@example.org");
        assert_eq!(event.attendees[0].response, "accept");

        let request = server.join().expect("server thread");
        assert!(request.contains("CalendarView"), "the server must expand series: {request}");

        server_assert_window(&request);
    }

    /// The window must reach the server as the dates asked for, or a day view
    /// would quietly show a different day.
    fn server_assert_window(request: &str) {
        assert!(request.contains("StartDate=\"2026-"), "missing start: {request}");
        assert!(request.contains("EndDate=\"2026-"), "missing end: {request}");
    }

    #[tokio::test]
    async fn a_second_sync_resumes_instead_of_re_enumerating() {
        // Three replies: the first round, its (empty) envelope follow-up, and
        // the second round.
        let empty_items = r#"<?xml version="1.0" encoding="utf-8"?>
<s:Envelope xmlns:s="http://schemas.xmlsoap.org/soap/envelope/"><s:Header/><s:Body>
  <m:GetItemResponse xmlns:m="http://schemas.microsoft.com/exchange/services/2006/messages" xmlns:t="http://schemas.microsoft.com/exchange/services/2006/types">
    <m:ResponseMessages><m:GetItemResponseMessage ResponseClass="Success">
      <m:ResponseCode>NoError</m:ResponseCode><m:Items/>
    </m:GetItemResponseMessage></m:ResponseMessages>
  </m:GetItemResponse></s:Body></s:Envelope>"#;
        let (url, server) = serve_soap(vec![
            THREE_ITEM_SYNC_RESPONSE.to_string(),
            empty_items.to_string(),
            THREE_ITEM_SYNC_RESPONSE.to_string(),
            empty_items.to_string(),
        ]);
        let (mut session, _db) = test_session(url);
        session
            .folders
            .insert("INBOX".to_string(), ("AAMkINBOX=".to_string(), None));

        session.fetch_recent("INBOX", 5).await.expect("first sync");
        session.fetch_recent("INBOX", 5).await.expect("second sync");

        let requests = server.join().expect("server thread");
        assert!(
            !requests[0].contains("<t:SyncState>"),
            "the first sync has no state to resume from: {}",
            requests[0]
        );
        // The state the first round returned must be sent back, or the server
        // would enumerate the whole folder again on every sync.
        assert!(
            requests[2].contains("c3RhdGU="),
            "the second sync must resume from the stored state: {}",
            requests[2]
        );
    }

    #[tokio::test]
    async fn folder_lookup_fails_loud_for_an_unknown_name() {
        let (url, server) = serve_soap(vec![
            MIXED_HIERARCHY_RESPONSE.to_string(),
            distinguished_response(),
        ]);
        let (mut session, _db) = test_session(url);

        let Err(error) = session.fetch_recent("No Such Folder", 10).await else {
            panic!("an unknown folder must not resolve");
        };
        assert!(
            error.to_string().contains("No Such Folder"),
            "unhelpful error: {error}"
        );

        server.join().expect("server thread");
    }

    #[test]
    fn session_uid_assignment_is_stable_across_syncs() {
        let db = test_db();
        let session = EwsSession::new(
            EwsConfig {
                url: "http://127.0.0.1:1".to_string(),
                username: "u".to_string(),
                password: "p".to_string(),
            },
            "acct",
            db,
        );

        let envelope = |id: &str| EwsEnvelope {
            item_id: id.to_string(),
            change_key: Some("CK".to_string()),
            subject: "s".to_string(),
            from_name: String::new(),
            from_addr: String::new(),
            date: 0,
            seen: true,
            message_id: String::new(),
            in_reply_to: String::new(),
            references_root: String::new(),
            to: Vec::new(),
            cc: Vec::new(),
        };

        let first = session
            .assign_uids("INBOX", vec![envelope("AAMkA="), envelope("AAMkB=")])
            .expect("assignment should succeed");
        assert_eq!(first.iter().map(|m| m.uid).collect::<Vec<_>>(), vec![1, 2]);
        assert_eq!(first[0].folder, "INBOX");

        // A later sync seeing the same items must reuse their uids, or every
        // cached row and open thread would point at the wrong message.
        let second = session
            .assign_uids("INBOX", vec![envelope("AAMkB="), envelope("AAMkC=")])
            .expect("assignment should succeed");
        assert_eq!(second.iter().map(|m| m.uid).collect::<Vec<_>>(), vec![2, 3]);
    }

    /// Serves one scripted SOAP reply per request, in order, then stops.
    /// Returns the raw requests for assertions. Same pattern as
    /// `one_shot_json_server` in protocol/tests.rs, kept local because it must
    /// serve XML — and must answer several requests, since one high-level
    /// operation can take more than one round trip.
    fn serve_soap(bodies: Vec<String>) -> (String, std::thread::JoinHandle<Vec<String>>) {
        use std::io::{Read as _, Write as _};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind");
        let addr = listener.local_addr().expect("addr");
        let handle = std::thread::spawn(move || {
            let mut requests = Vec::new();
            for body in bodies {
                let (mut stream, _) = listener.accept().expect("accept");
                let mut request = Vec::new();
                let mut chunk = [0u8; 4096];
                let (header_end, content_length) = loop {
                    let n = stream.read(&mut chunk).expect("read");
                    request.extend_from_slice(&chunk[..n]);
                    if let Some(pos) = request.windows(4).position(|w| w == b"\r\n\r\n") {
                        let headers = String::from_utf8_lossy(&request[..pos]).to_lowercase();
                        let length = headers
                            .lines()
                            .find_map(|l| l.strip_prefix("content-length:"))
                            .and_then(|v| v.trim().parse::<usize>().ok())
                            .unwrap_or(0);
                        break (pos + 4, length);
                    }
                };
                while request.len() < header_end + content_length {
                    let n = stream.read(&mut chunk).expect("read body");
                    request.extend_from_slice(&chunk[..n]);
                }
                let response = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: text/xml; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    body.len(),
                    body
                );
                stream.write_all(response.as_bytes()).expect("write");
                requests.push(String::from_utf8_lossy(&request).into_owned());
            }
            requests
        });
        (format!("http://{addr}"), handle)
    }

    /// A reply resolving the mailbox's well-known folders, as `list_folders`
    /// asks for right after the hierarchy round.
    fn distinguished_response() -> String {
        r#"<?xml version="1.0" encoding="utf-8"?>
<s:Envelope xmlns:s="http://schemas.xmlsoap.org/soap/envelope/">
  <s:Header/>
  <s:Body>
    <m:GetFolderResponse xmlns:m="http://schemas.microsoft.com/exchange/services/2006/messages" xmlns:t="http://schemas.microsoft.com/exchange/services/2006/types">
      <m:ResponseMessages>
        <m:GetFolderResponseMessage ResponseClass="Success">
          <m:ResponseCode>NoError</m:ResponseCode>
          <m:Folders><t:Folder><t:FolderId Id="AAMkINBOX=" ChangeKey="CK-INBOX"/></t:Folder></m:Folders>
        </m:GetFolderResponseMessage>
        <m:GetFolderResponseMessage ResponseClass="Success">
          <m:ResponseCode>NoError</m:ResponseCode>
          <m:Folders><t:Folder><t:FolderId Id="AAMkSENT=" ChangeKey="CK-SENT"/></t:Folder></m:Folders>
        </m:GetFolderResponseMessage>
        <m:GetFolderResponseMessage ResponseClass="Error">
          <m:MessageText>The specified folder could not be found in the store.</m:MessageText>
          <m:ResponseCode>ErrorFolderNotFound</m:ResponseCode>
        </m:GetFolderResponseMessage>
        <m:GetFolderResponseMessage ResponseClass="Error">
          <m:MessageText>The specified folder could not be found in the store.</m:MessageText>
          <m:ResponseCode>ErrorFolderNotFound</m:ResponseCode>
        </m:GetFolderResponseMessage>
        <m:GetFolderResponseMessage ResponseClass="Error">
          <m:MessageText>The specified folder could not be found in the store.</m:MessageText>
          <m:ResponseCode>ErrorFolderNotFound</m:ResponseCode>
        </m:GetFolderResponseMessage>
        <m:GetFolderResponseMessage ResponseClass="Error">
          <m:MessageText>The specified folder could not be found in the store.</m:MessageText>
          <m:ResponseCode>ErrorFolderNotFound</m:ResponseCode>
        </m:GetFolderResponseMessage>
      </m:ResponseMessages>
    </m:GetFolderResponse>
  </s:Body>
</s:Envelope>"#
            .to_string()
    }

    /// Single-request convenience over [`serve_soap`].
    fn serve_soap_once(body: &'static str) -> (String, std::thread::JoinHandle<String>) {
        let (url, handle) = serve_soap(vec![body.to_string()]);
        (
            url,
            std::thread::spawn(move || handle.join().expect("server thread").remove(0)),
        )
    }

    #[test]
    fn client_round_trips_folder_hierarchy_through_real_http() {
        let (url, server) = serve_soap_once(HIERARCHY_RESPONSE);
        let client = EwsClient::new(EwsConfig {
            url,
            username: "CORP\\user".to_string(),
            password: "pw".to_string(),
        });
        let round = client
            .folder_hierarchy(None)
            .expect("hierarchy sync should succeed");
        assert_eq!(round.changes.inner.len(), 1);
        let request = server.join().expect("server thread");
        // ureq lower-cases header names on the wire; the base64 value is
        // case-sensitive, so assert the two separately.
        assert!(
            request.to_lowercase().contains("authorization: basic"),
            "missing basic auth header: {request}"
        );
        assert!(
            request.contains("Q09SUFx1c2VyOnB3"),
            "missing encoded credentials: {request}"
        );
        assert!(request.contains("SyncFolderHierarchy"), "missing operation: {request}");
    }

    #[test]
    fn client_round_trips_mime_fetch_through_real_http() {
        let (url, server) = serve_soap_once(GET_ITEM_RESPONSE);
        let client = EwsClient::new(EwsConfig {
            url,
            username: "u".to_string(),
            password: "p".to_string(),
        });
        let fetched = client
            .fetch_mime(&[("AAMkMIME=".to_string(), None)])
            .expect("fetch should succeed");
        assert_eq!(fetched.len(), 1);
        assert_eq!(fetched[0].0.id, "AAMkMIME=");
        assert_eq!(fetched[0].1, b"Subject: hi\r\n\r\nbody");
        let request = server.join().expect("server thread");
        assert!(request.contains("IncludeMimeContent"), "missing MIME flag: {request}");
    }

    #[test]
    fn parse_response_reads_a_hierarchy_sync_round() {
        let response = parse_response::<SyncFolderHierarchy>(HIERARCHY_RESPONSE.as_bytes())
            .expect("parse should succeed");
        let message = single_message(into_successes(response).expect("response should be a success"))
            .expect("exactly one response message");
        assert_eq!(message.sync_state, "H4sIAAAAAAAEAO29B2AcSZY=");
        assert!(message.includes_last_folder_in_range);
        assert_eq!(message.changes.inner.len(), 1);
        match &message.changes.inner[0] {
            Change::Create { folder } => {
                // Folder details are asserted by shape; exact field mapping is
                // covered when the backend maps them to `imap::Folder`.
                let debug = format!("{folder:?}");
                assert!(debug.contains("Inbox"), "unexpected folder: {debug}");
            }
            other => panic!("expected a Create change, got {other:?}"),
        }
    }

    /// Two messages, returned in the opposite order to the request — which
    /// EWS is free to do.
    const TWO_MIME_RESPONSE: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<s:Envelope xmlns:s="http://schemas.xmlsoap.org/soap/envelope/">
  <s:Header/>
  <s:Body>
    <m:GetItemResponse xmlns:m="http://schemas.microsoft.com/exchange/services/2006/messages" xmlns:t="http://schemas.microsoft.com/exchange/services/2006/types">
      <m:ResponseMessages>
        <m:GetItemResponseMessage ResponseClass="Success">
          <m:ResponseCode>NoError</m:ResponseCode>
          <m:Items>
            <t:Message>
              <t:MimeContent CharacterSet="UTF-8">U3ViamVjdDogc2Vjb25kDQoNCmJvZHkgdHdv</t:MimeContent>
              <t:ItemId Id="AAMkB=" ChangeKey="CK-B"/>
            </t:Message>
            <t:Message>
              <t:MimeContent CharacterSet="UTF-8">U3ViamVjdDogZmlyc3QNCg0KYm9keSBvbmU=</t:MimeContent>
              <t:ItemId Id="AAMkA=" ChangeKey="CK-A"/>
            </t:Message>
          </m:Items>
        </m:GetItemResponseMessage>
      </m:ResponseMessages>
    </m:GetItemResponse>
  </s:Body>
</s:Envelope>"#;

    /// A success reply for a write operation. `payload` carries whatever the
    /// operation's response message is required to contain — MoveItem echoes
    /// an `Items` element, DeleteItem returns nothing.
    fn ok_response(operation: &str, payload: &str) -> String {
        format!(
            r#"<?xml version="1.0" encoding="utf-8"?>
<s:Envelope xmlns:s="http://schemas.xmlsoap.org/soap/envelope/">
  <s:Header/>
  <s:Body>
    <m:{operation}Response xmlns:m="http://schemas.microsoft.com/exchange/services/2006/messages" xmlns:t="http://schemas.microsoft.com/exchange/services/2006/types">
      <m:ResponseMessages>
        <m:{operation}ResponseMessage ResponseClass="Success">
          <m:ResponseCode>NoError</m:ResponseCode>
          {payload}
        </m:{operation}ResponseMessage>
      </m:ResponseMessages>
    </m:{operation}Response>
  </s:Body>
</s:Envelope>"#
        )
    }

    #[tokio::test]
    async fn fetch_bodies_pairs_each_message_with_its_own_uid() {
        let (url, server) = serve_soap_once(TWO_MIME_RESPONSE);
        let (mut session, db) = test_session(url);
        {
            let conn = db.lock().unwrap();
            assert_eq!(
                crate::store::map_ews_item(&conn, "acct", "INBOX", "AAMkA=", None).unwrap(),
                1
            );
            assert_eq!(
                crate::store::map_ews_item(&conn, "acct", "INBOX", "AAMkB=", None).unwrap(),
                2
            );
        }

        let bodies = session
            .fetch_bodies("INBOX", &[1, 2], std::env::temp_dir(), "acct")
            .await
            .expect("fetch should succeed");

        // The server answered B before A; pairing must follow item ids, not
        // response order, or every body would render under the wrong message.
        let by_uid: std::collections::HashMap<u32, String> = bodies
            .into_iter()
            .map(|(uid, message)| (uid, message.subject))
            .collect();
        assert_eq!(by_uid.get(&1).map(String::as_str), Some("first"));
        assert_eq!(by_uid.get(&2).map(String::as_str), Some("second"));

        server.join().expect("server thread");
    }

    #[tokio::test]
    async fn storing_a_flag_without_a_selected_folder_fails_loud() {
        let (mut session, db) = test_session("http://127.0.0.1:1".to_string());
        {
            let conn = db.lock().unwrap();
            crate::store::map_ews_item(&conn, "acct", "INBOX", "AAMkA=", None).unwrap();
        }

        // No prepare_flag_update ran, so there is no folder to resolve uids
        // against; silently doing nothing would leave the UI showing a flag
        // the server never received.
        let error = session
            .store_seen(&[1], true)
            .await
            .expect_err("a flag update with no selected folder must fail");
        assert!(
            error.to_string().contains("no folder selected"),
            "unhelpful error: {error}"
        );
    }

    #[tokio::test]
    async fn moving_messages_drops_their_source_mappings() {
        let (url, server) = serve_soap_once(Box::leak(
            ok_response("MoveItem", "<m:Items/>").into_boxed_str(),
        ));
        let (mut session, db) = test_session(url);
        {
            let conn = db.lock().unwrap();
            crate::store::map_ews_item(&conn, "acct", "INBOX", "AAMkA=", Some("CK-A")).unwrap();
        }
        // Pre-seed the folder map so the move needs no hierarchy round trip.
        session
            .folders
            .insert("Archive".to_string(), ("AAMkARCH=".to_string(), None));

        session
            .move_to_folder("INBOX", "Archive", &[1])
            .await
            .expect("move should succeed");

        // Exchange reissues item ids on move, so the source mapping is stale;
        // the destination's next sync mints a fresh uid for the arrival.
        let conn = db.lock().unwrap();
        assert_eq!(
            crate::store::ews_item_for_uid(&conn, "acct", "INBOX", 1).unwrap(),
            None
        );

        server.join().expect("server thread");
    }
}

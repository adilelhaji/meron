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
    PathToElement, RealItem, ResponseClass,
};

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
                                message: Message {
                                    is_read: Some(is_read),
                                    ..Message::default()
                                },
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
        parse_response::<Op>(&response)
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

fn basic_auth(username: &str, password: &str) -> String {
    let credentials =
        base64::engine::general_purpose::STANDARD.encode(format!("{username}:{password}"));
    format!("Basic {credentials}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use ::ews::sync_folder_hierarchy::Change;

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
            conflict_resolution: Some(ConflictResolution::AutoResolve),
            item_changes: vec![ItemChange {
                item_change: ItemChangeInner {
                    item_id: item_id(&("AAMk=".to_string(), Some("CQAA".to_string()))),
                    updates: Updates {
                        inner: vec![ItemChangeDescription::SetItemField {
                            field_uri: PathToElement::FieldURI {
                                field_URI: "message:IsRead".to_string(),
                            },
                            message: Message {
                                is_read: Some(true),
                                ..Message::default()
                            },
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

    /// Minimal single-request HTTP/1.1 server: accepts one connection, reads
    /// one request, replies 200 with `body`, and hands the raw request back
    /// for assertions. Same pattern as `one_shot_json_server` in
    /// protocol/tests.rs, kept local because it must serve XML.
    fn serve_soap_once(body: &'static str) -> (String, std::thread::JoinHandle<String>) {
        use std::io::{Read as _, Write as _};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind");
        let addr = listener.local_addr().expect("addr");
        let handle = std::thread::spawn(move || {
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
            String::from_utf8_lossy(&request).into_owned()
        });
        (format!("http://{addr}"), handle)
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
}

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

use ::ews::server_version::ExchangeServerVersion;
use ::ews::soap::{Envelope, Header};
use ::ews::sync_folder_hierarchy::{SyncFolderHierarchy, SyncFolderHierarchyResponseMessage};
use ::ews::{BaseFolderId, BaseShape, FolderShape, Operation, OperationResponse, ResponseClass};

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

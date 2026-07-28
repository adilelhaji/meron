//! OS keychain storage for per-account secrets (IMAP password + OAuth tokens).
//!
//! Secrets live in the platform keychain — Keychain on macOS, Credential Manager
//! on Windows, and the Secret Service (D-Bus, e.g. gnome-keyring) on native Linux.
//! Inside Flatpak, oo7 uses the Secret portal to encrypt an app-private keyring.
//! One entry per account holds a JSON blob; non-secret connection metadata (host,
//! port, user, ...) stays in SQLite.

use anyhow::{Context, Result};
use keyring::Entry;
use serde::{Deserialize, Serialize};

use crate::imap::Creds;

// OS keychain service name, matching the app name.
const SERVICE: &str = "meron";

// Reserved keychain "account" holding the SQLCipher key for the local store.
// The leading underscores keep it out of the real per-account id namespace.
const DB_KEY_ACCOUNT: &str = "__meron_db_key__";

#[derive(Default, Clone, Serialize, Deserialize)]
pub struct Secrets {
    #[serde(default)]
    pub password: String,
    #[serde(default)]
    pub access_token: Option<String>,
    #[serde(default)]
    pub refresh_token: Option<String>,
}

impl Secrets {
    /// Pull the secret-bearing fields out of a fully-populated `Creds`.
    pub fn from_creds(c: &Creds) -> Self {
        Secrets {
            password: c.password.clone(),
            access_token: c.access_token.clone().filter(|s| !s.is_empty()),
            refresh_token: c.refresh_token.clone().filter(|s| !s.is_empty()),
        }
    }

    /// Overlay these secrets onto a `Creds` loaded from the (secret-free) store.
    pub fn apply_to(&self, c: &mut Creds) {
        c.password = self.password.clone();
        c.access_token = self.access_token.clone();
        c.refresh_token = self.refresh_token.clone();
    }

    pub fn is_empty(&self) -> bool {
        self.password.is_empty()
            && self.access_token.as_deref().unwrap_or("").is_empty()
            && self.refresh_token.as_deref().unwrap_or("").is_empty()
    }
}

/// `MERON_KEYRING` escape hatches, all opt-in and none set in normal use:
///
///   * `off` — no keychain at all: operations become no-ops and `load` returns
///     empty secrets (tests/headless CI). Within a single sidecar run secrets
///     stay in memory, so only cross-restart persistence is lost.
///   * `service` — force the D-Bus Secret Service even inside Flatpak, for
///     sandboxes that can reach the host service directly.
///   * `file` — force the local file keyring, skipping D-Bus entirely. The
///     supported workaround when a desktop has no working secret storage.
fn keyring_disabled() -> bool {
    std::env::var_os("MERON_KEYRING").is_some_and(|v| v == "off")
}

// Linux talks to the keychain through `backend_*` below, which builds its own
// entries inside the timeout wrapper.
#[cfg(not(target_os = "linux"))]
fn entry(account: &str) -> Result<Entry> {
    Entry::new(SERVICE, account).with_context(|| format!("open keychain entry for {account}"))
}

#[cfg(target_os = "linux")]
fn keyring_forced_to(mode: &str) -> bool {
    std::env::var_os("MERON_KEYRING").is_some_and(|v| v == mode)
}

#[cfg(target_os = "linux")]
fn use_portal_keyring() -> bool {
    if keyring_forced_to("service") {
        return false;
    }
    flatpak_detected(
        std::env::var_os("FLATPAK_ID").as_deref(),
        std::path::Path::new("/.flatpak-info").exists(),
    )
}

/// Whether to go through [`crate::secrets_portal`] (Secret portal, or its local
/// file fallback) rather than the D-Bus Secret Service.
#[cfg(target_os = "linux")]
fn use_oo7_keyring() -> bool {
    use_portal_keyring()
        || keyring_forced_to("file")
        || crate::secrets_portal::local_forced()
        || (!keyring_forced_to("service") && crate::secrets_portal::local_keyring_exists())
}

#[cfg(target_os = "linux")]
fn flatpak_detected(flatpak_id: Option<&std::ffi::OsStr>, marker_exists: bool) -> bool {
    flatpak_id.is_some_and(|id| !id.is_empty()) || marker_exists
}

#[cfg(not(target_os = "linux"))]
fn use_portal_keyring() -> bool {
    false
}

/// Store (or replace) an account's secrets in the OS keychain.
pub fn store(account: &str, secrets: &Secrets) -> Result<()> {
    if keyring_disabled() {
        return Ok(());
    }
    let blob = serde_json::to_string(secrets)?;
    backend_store(account, &blob)
}

/// Load an account's secrets, or defaults if no entry exists.
pub fn load(account: &str) -> Result<Secrets> {
    if keyring_disabled() {
        return Ok(Secrets::default());
    }
    match backend_load(account)? {
        Some(blob) => Ok(serde_json::from_str(&blob).unwrap_or_default()),
        None => Ok(Secrets::default()),
    }
}

// ---- Backends ---------------------------------------------------------------
//
// On Linux the keychain is a D-Bus service that may be absent (a Flatpak with
// no Secret portal backend, a Snap whose `password-manager-service` interface
// was never connected) and, when it is, may never answer. Every call is
// therefore bounded, and an unavailable service demotes the process to the
// local file keyring instead of failing every operation for the rest of the
// run. macOS and Windows keep the plain synchronous path.

#[cfg(target_os = "linux")]
fn backend_store(account: &str, blob: &str) -> Result<()> {
    let owned_account = account.to_owned();
    let owned_blob = blob.to_owned();
    with_service_fallback(
        move || {
            service_call("write", move || {
                Entry::new(SERVICE, &owned_account)?.set_password(&owned_blob)
            })
        },
        || {
            crate::secrets_portal::store(account, blob)
                .with_context(|| format!("write keyring entry for {account}"))
        },
    )
}

#[cfg(not(target_os = "linux"))]
fn backend_store(account: &str, blob: &str) -> Result<()> {
    entry(account)?
        .set_password(blob)
        .with_context(|| format!("write keychain entry for {account}"))
}

#[cfg(target_os = "linux")]
fn backend_load(account: &str) -> Result<Option<String>> {
    let owned_account = account.to_owned();
    with_service_fallback(
        move || {
            service_call("read", move || {
                match Entry::new(SERVICE, &owned_account)?.get_password() {
                    Ok(blob) => Ok(Some(blob)),
                    Err(keyring::Error::NoEntry) => Ok(None),
                    Err(error) => Err(error),
                }
            })
        },
        || {
            crate::secrets_portal::load(account)
                .with_context(|| format!("read keyring entry for {account}"))
        },
    )
}

#[cfg(not(target_os = "linux"))]
fn backend_load(account: &str) -> Result<Option<String>> {
    match entry(account)?.get_password() {
        Ok(blob) => Ok(Some(blob)),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(e) => Err(e).with_context(|| format!("read keychain entry for {account}")),
    }
}

#[cfg(target_os = "linux")]
fn backend_delete(account: &str) -> Result<()> {
    let owned_account = account.to_owned();
    with_service_fallback(
        move || {
            service_call("delete", move || {
                match Entry::new(SERVICE, &owned_account)?.delete_credential() {
                    Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
                    Err(error) => Err(error),
                }
            })
        },
        || {
            crate::secrets_portal::delete(account)
                .with_context(|| format!("delete keyring entry for {account}"))
        },
    )
}

#[cfg(not(target_os = "linux"))]
fn backend_delete(account: &str) -> Result<()> {
    match entry(account)?.delete_credential() {
        Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
        Err(e) => Err(e).with_context(|| format!("delete keychain entry for {account}")),
    }
}

/// A failed Secret Service call. `unavailable` separates "there is no usable
/// service here" — worth falling back for — from a real error such as an
/// ambiguous or oversized entry, which the local keyring would not fix.
#[cfg(target_os = "linux")]
struct ServiceFailure {
    error: anyhow::Error,
    unavailable: bool,
}

#[cfg(target_os = "linux")]
impl ServiceFailure {
    fn unavailable(message: String) -> Self {
        Self {
            error: anyhow::anyhow!(message),
            unavailable: true,
        }
    }

    fn from_keyring(what: &str, error: keyring::Error) -> Self {
        let unavailable = matches!(
            error,
            keyring::Error::NoStorageAccess(_) | keyring::Error::PlatformFailure(_)
        );
        Self {
            error: anyhow::Error::new(error).context(format!("Secret Service {what}")),
            unavailable,
        }
    }
}

/// Run one blocking Secret Service call on a throwaway thread, bounded by
/// [`SERVICE_TIMEOUT`]. The thread is abandoned on timeout rather than joined:
/// a wedged D-Bus call never returns, and the sticky demotion means we stop
/// issuing these calls after the first one gives up.
#[cfg(target_os = "linux")]
const SERVICE_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(10);

#[cfg(target_os = "linux")]
fn service_call<T: Send + 'static>(
    what: &str,
    call: impl FnOnce() -> std::result::Result<T, keyring::Error> + Send + 'static,
) -> std::result::Result<T, ServiceFailure> {
    let (sender, receiver) = std::sync::mpsc::channel();
    std::thread::Builder::new()
        .name("meron-secret-service".into())
        .spawn(move || {
            let _ = sender.send(call());
        })
        .map_err(|error| ServiceFailure::unavailable(format!("spawn keychain thread: {error}")))?;
    match receiver.recv_timeout(SERVICE_TIMEOUT) {
        Ok(Ok(value)) => Ok(value),
        Ok(Err(error)) => Err(ServiceFailure::from_keyring(what, error)),
        Err(_) => Err(ServiceFailure::unavailable(format!(
            "Secret Service {what} timed out after {}s",
            SERVICE_TIMEOUT.as_secs()
        ))),
    }
}

/// Try the Secret Service, falling back to the oo7 keyring (portal or local
/// file) when there is no usable service. Once demoted, later calls skip the
/// service entirely.
#[cfg(target_os = "linux")]
fn with_service_fallback<T>(
    service: impl FnOnce() -> std::result::Result<T, ServiceFailure>,
    local: impl FnOnce() -> Result<T>,
) -> Result<T> {
    if use_oo7_keyring() {
        return local();
    }
    match service() {
        Ok(value) => Ok(value),
        Err(failure) if failure.unavailable => {
            crate::secrets_portal::force_local(&format!("{:#}", failure.error));
            local()
        }
        Err(failure) => Err(failure.error),
    }
}

/// The SQLCipher key for the local store (64 hex chars = a raw 32-byte key),
/// created and persisted in the OS keychain on first use.
///
/// Returns `Ok(None)` when the keychain is disabled (`MERON_KEYRING=off`,
/// tests/headless), so the store falls back to plaintext exactly as before.
pub fn db_key() -> Result<Option<String>> {
    if keyring_disabled() {
        return Ok(None);
    }
    match backend_load(DB_KEY_ACCOUNT).context("read db key from keychain")? {
        Some(key) if is_hex_key(&key) => Ok(Some(key)),
        // Missing (first run) or a malformed entry: mint and persist a fresh key.
        Some(_) | None => {
            let key = generate_db_key();
            backend_store(DB_KEY_ACCOUNT, &key).context("store db key in keychain")?;
            Ok(Some(key))
        }
    }
}

/// 32 random bytes (two v4 UUIDs' worth) rendered as 64 lowercase hex chars.
fn generate_db_key() -> String {
    use std::fmt::Write;
    let mut hex = String::with_capacity(64);
    for _ in 0..2 {
        for byte in uuid::Uuid::new_v4().as_bytes() {
            let _ = write!(hex, "{byte:02x}");
        }
    }
    hex
}

fn is_hex_key(s: &str) -> bool {
    s.len() == 64 && s.bytes().all(|b| b.is_ascii_hexdigit())
}

/// Remove an account's secrets from the keychain. A missing entry is not an error.
pub fn delete(account: &str) -> Result<()> {
    if keyring_disabled() {
        return Ok(());
    }
    backend_delete(account)
}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use super::flatpak_detected;
    use std::ffi::OsStr;

    #[test]
    fn flatpak_detection_does_not_match_native_linux() {
        assert!(!flatpak_detected(None, false));
        assert!(!flatpak_detected(Some(OsStr::new("")), false));
        assert!(flatpak_detected(
            Some(OsStr::new("jp.nonbili.meron")),
            false
        ));
        assert!(flatpak_detected(None, true));
    }
}

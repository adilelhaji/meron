use super::*;

/// Mobile `backup.export`. Mirrors the desktop handler in `main.rs`, differing
/// only in where secrets live: the keyed app-private DB rather than an OS
/// keychain.
pub(crate) fn export_mobile_backup(data_dir: &str, params: &Value) -> Result<Value, String> {
    let include_secrets = params
        .get("include_secrets")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let passphrase = opt_str(params, "passphrase");
    with_mobile_db(data_dir, |conn| {
        let text = crate::backup::export(&conn, include_secrets, Some(&passphrase), &|account| {
            load_mobile_secret(&conn, account)
        })
        .map_err(|err| format!("{err:#}"))?;
        Ok(json!({ "backup": text }))
    })
}

/// Mobile `backup.import`. An encrypted file opened without a passphrase comes
/// back as `{ "needs_passphrase": true }` rather than an error, so the host can
/// prompt and call again.
pub(crate) fn import_mobile_backup(data_dir: &str, params: &Value) -> Result<Value, String> {
    let text = req_str(params, "backup")?;
    let passphrase = opt_str(params, "passphrase");
    with_mobile_db(data_dir, |conn| {
        let data = match crate::backup::parse(&text, Some(&passphrase)) {
            Ok(data) => data,
            Err(err) if crate::backup::needs_passphrase(&err.to_string()) => {
                return Ok(json!({ "needs_passphrase": true }));
            }
            Err(err) => return Err(format!("{err:#}")),
        };
        let summary = crate::backup::apply(&conn, &data, &|conn, account, secrets| {
            let blob = serde_json::to_string(secrets)?;
            store::upsert_secret(conn, account, &blob)
        })
        .map_err(|err| format!("{err:#}"))?;
        // Restored settings include the app-wide proxy, which socket code reads
        // from a process-global slot rather than the DB.
        crate::proxy::load_global(&conn).map_err(|err| format!("{err:#}"))?;
        Ok(summary.to_json())
    })
}

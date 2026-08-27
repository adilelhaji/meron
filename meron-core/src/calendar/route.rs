//! Which backend serves an account's calendar.
//!
//! Calendars are deliberately *not* dispatched through the mail `Session`
//! enum. An account's mail and its calendar are different protocols with
//! different lifetimes: Exchange serves both over one pooled SOAP session,
//! while Gmail's mail arrives over IMAP — which has no calendar at all — and
//! its calendar over a stateless REST API. Routing calendars through the mail
//! session would have forced either a second session per account or a
//! `Session::Google` that cannot speak mail.
//!
//! Every function here takes the account and answers for whichever backend
//! serves it, so callers never ask what kind of account they hold.

use anyhow::Result;
use std::sync::Arc;

use super::{Calendar, Event};
use crate::engine::Engine;

/// What serves an account's calendar.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Route {
    /// Exchange, over the account's pooled EWS session.
    Exchange,
    /// Google Calendar, over REST with the account's OAuth token.
    Google,
    /// Nothing: the account has no calendar to serve. Plain IMAP, and a Google
    /// account signed in with an app password rather than with Google — an app
    /// password authenticates mail only, and grants no API access.
    None,
}

impl Route {
    pub fn of(creds: &crate::imap::Creds) -> Self {
        if creds.is_ews() {
            return Route::Exchange;
        }
        if creds.auth_type == "gmail_oauth" {
            return Route::Google;
        }
        Route::None
    }
}

/// The route an account's calendar takes, with a valid access token when the
/// route needs one. Resolving credentials also refreshes an expired token.
async fn route_with_token(engine: &Arc<Engine>, account: &str) -> Result<(Route, String)> {
    let creds = engine.ensure_valid_creds(account).await?;
    let route = Route::of(&creds);
    let token = match route {
        Route::Google => creds.access_token.clone().unwrap_or_default(),
        _ => String::new(),
    };
    if route == Route::Google && token.is_empty() {
        anyhow::bail!("account needs reconnect: {account}");
    }
    Ok((route, token))
}

/// Runs a Google Calendar call off the async runtime, since the HTTP client is
/// blocking like the rest of this crate's.
async fn blocking<T, F>(work: F) -> Result<T>
where
    F: FnOnce() -> Result<T> + Send + 'static,
    T: Send + 'static,
{
    tokio::task::spawn_blocking(work).await?
}

pub async fn list_calendars(engine: &Arc<Engine>, account: &str) -> Result<Vec<Calendar>> {
    match route_with_token(engine, account).await? {
        (Route::Exchange, _) => {
            engine
                .with_read_session(account, |session| {
                    Box::pin(async move { session.list_calendars().await })
                })
                .await
        }
        (Route::Google, token) => blocking(move || super::google::list_calendars(&token)).await,
        // An account without a calendar reports none rather than failing: a
        // mail account that simply has no calendar is not an error to show.
        (Route::None, _) => Ok(Vec::new()),
    }
}

pub async fn events_in_window(
    engine: &Arc<Engine>,
    account: &str,
    calendar_id: &str,
    from: i64,
    to: i64,
) -> Result<Vec<Event>> {
    match route_with_token(engine, account).await? {
        (Route::Exchange, _) => {
            let id = calendar_id.to_string();
            engine
                .with_read_session(account, |session| {
                    let id = id.clone();
                    Box::pin(async move { session.events_in_window(&id, from, to).await })
                })
                .await
        }
        (Route::Google, token) => {
            let id = calendar_id.to_string();
            blocking(move || super::google::events_in_window(&token, &id, from, to)).await
        }
        (Route::None, _) => Ok(Vec::new()),
    }
}

pub async fn create_event(engine: &Arc<Engine>, account: &str, event: &Event) -> Result<Event> {
    match route_with_token(engine, account).await? {
        (Route::Exchange, _) => {
            let event = event.clone();
            engine
                .with_write_session(account, |session| {
                    let event = event.clone();
                    Box::pin(async move { session.create_event(&event).await })
                })
                .await
        }
        (Route::Google, token) => {
            let event = event.clone();
            blocking(move || super::google::create_event(&token, &event)).await
        }
        (Route::None, _) => anyhow::bail!("this account has no calendar"),
    }
}

/// Applies a changed event to the occurrence alone, or to its whole series.
pub async fn update_event(
    engine: &Arc<Engine>,
    account: &str,
    event: &Event,
    whole_series: bool,
) -> Result<()> {
    match route_with_token(engine, account).await? {
        (Route::Exchange, _) => {
            let event = event.clone();
            engine
                .with_write_session(account, |session| {
                    let event = event.clone();
                    Box::pin(async move { session.update_event(&event, whole_series).await })
                })
                .await
        }
        (Route::Google, token) => {
            let event = event.clone();
            blocking(move || super::google::update_event(&token, &event, whole_series)).await
        }
        (Route::None, _) => anyhow::bail!("this account has no calendar"),
    }
}

/// Deletes an occurrence, or the whole series it belongs to.
///
/// `series_id` is what Google needs to reach the master; Exchange finds it
/// from the occurrence's own id.
pub async fn delete_event(
    engine: &Arc<Engine>,
    account: &str,
    calendar_id: &str,
    event_id: &str,
    change_key: Option<&str>,
    series_id: Option<&str>,
    whole_series: bool,
) -> Result<()> {
    match route_with_token(engine, account).await? {
        (Route::Exchange, _) => {
            let id = event_id.to_string();
            let change_key = change_key.map(str::to_string);
            engine
                .with_write_session(account, |session| {
                    let id = id.clone();
                    let change_key = change_key.clone();
                    Box::pin(async move {
                        session
                            .delete_event(&id, change_key.as_deref(), whole_series)
                            .await
                    })
                })
                .await
        }
        (Route::Google, token) => {
            let calendar_id = calendar_id.to_string();
            // Deleting a whole series means deleting the master; an occurrence
            // is deleted by its own id, which leaves the rest standing.
            let target = match (whole_series, series_id) {
                (true, Some(series)) => series.to_string(),
                _ => event_id.to_string(),
            };
            blocking(move || super::google::delete_event(&token, &calendar_id, &target)).await
        }
        (Route::None, _) => anyhow::bail!("this account has no calendar"),
    }
}

/// The rule behind the series an occurrence belongs to, or `None` when there
/// is none to show.
///
/// Fetched when a reader opens a repeating event, rather than stored: the
/// store keeps occurrences, and a second copy of the rule would be a second
/// truth to go stale.
pub async fn series_rule(
    engine: &Arc<Engine>,
    account: &str,
    calendar_id: &str,
    event_id: &str,
    change_key: Option<&str>,
    series_id: Option<&str>,
) -> Result<Option<super::Recurrence>> {
    match route_with_token(engine, account).await? {
        (Route::Exchange, _) => {
            let id = event_id.to_string();
            let change_key = change_key.map(str::to_string);
            engine
                .with_read_session(account, |session| {
                    let id = id.clone();
                    let change_key = change_key.clone();
                    Box::pin(async move { session.series_rule(&id, change_key.as_deref()).await })
                })
                .await
        }
        (Route::Google, token) => {
            let Some(series) = series_id.map(str::to_string) else {
                return Ok(None);
            };
            let calendar_id = calendar_id.to_string();
            blocking(move || super::google::series_rule(&token, &calendar_id, &series)).await
        }
        (Route::None, _) => Ok(None),
    }
}

pub async fn create_calendar(engine: &Arc<Engine>, account: &str, name: &str) -> Result<String> {
    match route_with_token(engine, account).await? {
        (Route::Exchange, _) => {
            let name = name.to_string();
            engine
                .with_write_session(account, |session| {
                    let name = name.clone();
                    Box::pin(async move { session.create_calendar(&name).await })
                })
                .await
        }
        (Route::Google, token) => {
            let name = name.to_string();
            blocking(move || super::google::create_calendar(&token, &name)).await
        }
        (Route::None, _) => anyhow::bail!("this account has no calendar"),
    }
}

pub async fn rename_calendar(
    engine: &Arc<Engine>,
    account: &str,
    calendar_id: &str,
    name: &str,
) -> Result<()> {
    match route_with_token(engine, account).await? {
        (Route::Exchange, _) => {
            let id = calendar_id.to_string();
            let name = name.to_string();
            engine
                .with_write_session(account, |session| {
                    let id = id.clone();
                    let name = name.clone();
                    Box::pin(async move { session.rename_calendar(&id, &name).await })
                })
                .await
        }
        (Route::Google, token) => {
            let id = calendar_id.to_string();
            let name = name.to_string();
            blocking(move || super::google::rename_calendar(&token, &id, &name)).await
        }
        (Route::None, _) => anyhow::bail!("this account has no calendar"),
    }
}

pub async fn delete_calendar(
    engine: &Arc<Engine>,
    account: &str,
    calendar_id: &str,
    owned: bool,
) -> Result<()> {
    match route_with_token(engine, account).await? {
        (Route::Exchange, _) => {
            let id = calendar_id.to_string();
            engine
                .with_write_session(account, |session| {
                    let id = id.clone();
                    Box::pin(async move { session.delete_calendar(&id).await })
                })
                .await
        }
        (Route::Google, token) => {
            let id = calendar_id.to_string();
            blocking(move || super::google::delete_calendar(&token, &id, owned)).await
        }
        (Route::None, _) => anyhow::bail!("this account has no calendar"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn creds(auth_type: &str, ews_url: &str) -> crate::imap::Creds {
        crate::imap::Creds {
            host: "imap.example.com".to_string(),
            port: 993,
            user: "user@example.com".to_string(),
            password: String::new(),
            tls: true,
            starttls: false,
            smtp_host: "smtp.example.com".to_string(),
            smtp_port: 587,
            smtp_tls: true,
            smtp_starttls: false,
            auth_type: auth_type.to_string(),
            access_token: None,
            refresh_token: None,
            token_expires_at: 0,
            oauth_client_id: String::new(),
            oauth_client_secret: String::new(),
            oauth_token_url: String::new(),
            oauth_scope: String::new(),
            proxy: crate::proxy::ProxyChoice::default(),
            cert_pin: None,
            smtp_cert_pin: None,
            ews_url: ews_url.to_string(),
        }
    }

    #[test]
    fn an_account_is_routed_by_what_actually_serves_its_calendar() {
        assert_eq!(
            Route::of(&creds("password", "https://mail.example.org/EWS/Exchange.asmx")),
            Route::Exchange,
        );
        assert_eq!(Route::of(&creds("gmail_oauth", "")), Route::Google);

        // A Google account signed in with an app password authenticates mail
        // only: there is no token to call the Calendar API with, so it has no
        // calendar rather than a broken one.
        assert_eq!(Route::of(&creds("password", "")), Route::None);
        assert_eq!(Route::of(&creds("outlook_oauth", "")), Route::None);
    }
}

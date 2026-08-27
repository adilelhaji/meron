//! Google Calendar, over the REST API v3.
//!
//! Exchange speaks SOAP through a pooled session; Google speaks JSON over
//! plain HTTPS with a bearer token and keeps nothing between calls. That is
//! why calendars are not dispatched through the mail `Session`: an account's
//! mail and its calendar are different protocols with different lifetimes, and
//! Gmail's mail arrives over IMAP, which has no calendar at all.
//!
//! Recurrence is expanded by the server, as everywhere else in this codebase:
//! `singleEvents=true` asks Google to return concrete instances rather than
//! the rules behind them. See [`crate::calendar::subscription`] for the one
//! place that has no server to ask.

use anyhow::{Context as _, Result};
use serde::Deserialize;

use super::{Calendar, CalendarKind, Event, Participant};

const API: &str = "https://www.googleapis.com/calendar/v3";

/// Cap on one request. Generous enough for a busy month, short enough that a
/// hung connection does not wedge a sync.
const TIMEOUT: std::time::Duration = std::time::Duration::from_secs(60);

/// Cap on the occurrences one window may return. A window is a screenful of
/// calendar, not an archive.
const MAX_EVENTS: usize = 2500;

// ---- Wire types -------------------------------------------------------------
//
// Only the fields this client uses are modelled. Google adds fields over time
// and serde ignores the rest, so an unknown one is not an error.

#[derive(Deserialize)]
struct CalendarList {
    #[serde(default)]
    items: Vec<CalendarListEntry>,
    #[serde(rename = "nextPageToken")]
    next_page_token: Option<String>,
}

#[derive(Deserialize)]
struct CalendarListEntry {
    id: String,
    #[serde(default)]
    summary: String,
    #[serde(default)]
    primary: bool,
    /// "owner", "writer", "reader" or "freeBusyReader". Anything below writer
    /// cannot be edited — a calendar shared with this mailbox, or a public one.
    #[serde(rename = "accessRole", default)]
    access_role: String,
    #[serde(default)]
    deleted: bool,
    /// The colour this calendar is drawn in on Google's own clients, as
    /// `#rrggbb`. Worth carrying: a reader who has told Google that work is
    /// green should not have to say it again here.
    #[serde(rename = "backgroundColor", default)]
    background_color: Option<String>,
}

#[derive(Deserialize)]
struct EventList {
    #[serde(default)]
    items: Vec<GoogleEvent>,
    #[serde(rename = "nextPageToken")]
    next_page_token: Option<String>,
}

#[derive(Deserialize)]
struct GoogleEvent {
    #[serde(default)]
    id: String,
    #[serde(default)]
    etag: Option<String>,
    #[serde(default)]
    status: String,
    #[serde(default)]
    summary: Option<String>,
    #[serde(default)]
    location: Option<String>,
    start: Option<EventDateTime>,
    end: Option<EventDateTime>,
    #[serde(rename = "recurringEventId", default)]
    recurring_event_id: Option<String>,
    #[serde(default)]
    transparency: Option<String>,
    #[serde(default)]
    organizer: Option<GooglePerson>,
    #[serde(default)]
    attendees: Vec<GoogleAttendee>,
}

#[derive(Deserialize)]
struct EventDateTime {
    /// Set on a timed event, RFC 3339 with an offset.
    #[serde(rename = "dateTime")]
    date_time: Option<String>,
    /// Set on an all-day event: a calendar date with no time at all.
    date: Option<String>,
}

#[derive(Deserialize)]
struct GooglePerson {
    #[serde(default)]
    email: Option<String>,
    #[serde(rename = "displayName", default)]
    display_name: Option<String>,
}

#[derive(Deserialize)]
struct GoogleAttendee {
    #[serde(default)]
    email: Option<String>,
    #[serde(rename = "displayName", default)]
    display_name: Option<String>,
    #[serde(rename = "responseStatus", default)]
    response_status: Option<String>,
    /// Whether this attendee is the mailbox asking.
    #[serde(rename = "self", default)]
    is_self: bool,
}

#[derive(Deserialize)]
struct CreatedEvent {
    #[serde(default)]
    id: String,
    #[serde(default)]
    etag: Option<String>,
}

#[derive(Deserialize)]
struct CreatedCalendar {
    #[serde(default)]
    id: String,
}

// ---- Requests ---------------------------------------------------------------

/// One authenticated call. Blocking, like the rest of this crate's HTTP;
/// callers wrap it in `spawn_blocking`.
fn call(token: &str, method: &str, url: &str, body: Option<serde_json::Value>) -> Result<String> {
    let agent = crate::proxy::agent()?;
    let authorization = format!("Bearer {token}");

    // A request that carries a body and one that does not are different types
    // in the HTTP client, so the two paths are built separately rather than
    // through one branch. Both keep a non-2xx as a response rather than a
    // transport error, so the sentence Google puts in the body reaches the
    // user instead of a bare status code.
    // Serialised here rather than through the client's own JSON helper: that
    // helper is behind a crate feature this project does not enable, and one
    // less feature flag is worth one line of code.
    let payload = match &body {
        Some(json) => serde_json::to_string(json).context("serialise request")?,
        None => String::new(),
    };

    let mut response = match (method, body) {
        ("GET", _) => agent
            .get(url)
            .header("Authorization", &authorization)
            .config()
            .http_status_as_error(false)
            .timeout_global(Some(TIMEOUT))
            .build()
            .call()
            .with_context(|| format!("{method} {url}"))?,
        ("DELETE", _) => agent
            .delete(url)
            .header("Authorization", &authorization)
            .config()
            .http_status_as_error(false)
            .timeout_global(Some(TIMEOUT))
            .build()
            .call()
            .with_context(|| format!("{method} {url}"))?,
        ("POST", Some(_)) => agent
            .post(url)
            .header("Authorization", &authorization)
            .header("Content-Type", "application/json")
            .config()
            .http_status_as_error(false)
            .timeout_global(Some(TIMEOUT))
            .build()
            .send(payload.as_str())
            .with_context(|| format!("{method} {url}"))?,
        ("PATCH", Some(_)) => agent
            .patch(url)
            .header("Authorization", &authorization)
            .header("Content-Type", "application/json")
            .config()
            .http_status_as_error(false)
            .timeout_global(Some(TIMEOUT))
            .build()
            .send(payload.as_str())
            .with_context(|| format!("{method} {url}"))?,
        (other, _) => anyhow::bail!("unsupported request {other} {url}"),
    };

    let status = response.status();
    let text = response
        .body_mut()
        .read_to_string()
        .with_context(|| format!("read {method} {url}"))?;
    if !status.is_success() {
        anyhow::bail!("{}", explain(status.as_u16(), &text));
    }
    Ok(text)
}

/// What to tell the user about a refused call.
///
/// The one refusal worth naming is a token that predates the calendar
/// permission: the account works, the calendar does not, and nothing the user
/// can see says why. Reconnecting is the fix, so the message says so instead
/// of repeating Google's wording about scopes.
fn explain(status: u16, body: &str) -> String {
    let message = brief(body);
    let needs_consent = status == 403 && message.to_lowercase().contains("scope")
        || body.contains("ACCESS_TOKEN_SCOPE_INSUFFICIENT");
    if needs_consent {
        return "this account has not granted calendar access yet — reconnect it \
                to sign in again and allow the calendar"
            .to_string();
    }
    format!("Google Calendar answered {status}: {message}")
}

/// The human part of an API error, when there is one. Google answers with a
/// JSON envelope; showing the whole thing buries the sentence that matters.
fn brief(body: &str) -> String {
    #[derive(Deserialize)]
    struct ErrorEnvelope {
        error: Option<ErrorBody>,
    }
    #[derive(Deserialize)]
    struct ErrorBody {
        message: Option<String>,
    }
    serde_json::from_str::<ErrorEnvelope>(body)
        .ok()
        .and_then(|envelope| envelope.error.and_then(|error| error.message))
        .unwrap_or_else(|| body.chars().take(200).collect())
}

/// The calendars this account can see.
pub fn list_calendars(token: &str) -> Result<Vec<Calendar>> {
    let mut calendars = Vec::new();
    let mut page: Option<String> = None;
    loop {
        let url = match &page {
            Some(token) => format!("{API}/users/me/calendarList?maxResults=250&pageToken={token}"),
            None => format!("{API}/users/me/calendarList?maxResults=250"),
        };
        let list: CalendarList =
            serde_json::from_str(&call(token, "GET", &url, None)?).context("parse calendar list")?;
        for entry in list.items {
            // A calendar the account removed from its list is not one of its
            // own any more, the same way a deleted Exchange folder is not.
            if entry.deleted {
                continue;
            }
            calendars.push(Calendar {
                id: entry.id,
                name: entry.summary,
                is_default: entry.primary,
                // Both are this client's to decide and are preserved across
                // syncs by the store.
                enabled: true,
                color: entry
                    .background_color
                    .filter(|colour| colour.starts_with('#') && colour.len() == 7),
                kind: CalendarKind::Account,
                url: None,
                // Anything below "writer" is someone else's calendar shared
                // with this one: showing an edit that the server will refuse
                // would be worse than saying it cannot be edited.
                read_only: !matches!(entry.access_role.as_str(), "owner" | "writer"),
                synced_at: 0,
            });
        }
        match list.next_page_token {
            Some(next) => page = Some(next),
            None => break,
        }
    }
    Ok(calendars)
}

/// The occurrences on a calendar between two instants, series expanded.
pub fn events_in_window(
    token: &str,
    calendar_id: &str,
    from: i64,
    to: i64,
) -> Result<Vec<Event>> {
    let time_min = rfc3339(from).context("window start")?;
    let time_max = rfc3339(to).context("window end")?;
    let mut events = Vec::new();
    let mut page: Option<String> = None;
    loop {
        let mut url = format!(
            "{API}/calendars/{}/events?singleEvents=true&orderBy=startTime\
             &maxResults=250&showDeleted=false&timeMin={}&timeMax={}",
            urlencode(calendar_id),
            urlencode(&time_min),
            urlencode(&time_max),
        );
        if let Some(token) = &page {
            url.push_str("&pageToken=");
            url.push_str(&urlencode(token));
        }
        let list: EventList =
            serde_json::from_str(&call(token, "GET", &url, None)?).context("parse events")?;
        for source in list.items {
            if let Some(event) = to_event(source, calendar_id) {
                events.push(event);
            }
        }
        if events.len() >= MAX_EVENTS {
            break;
        }
        match list.next_page_token {
            Some(next) => page = Some(next),
            None => break,
        }
    }
    Ok(events)
}

/// Creates an event and returns it with the id the server assigned.
pub fn create_event(token: &str, event: &Event) -> Result<Event> {
    let url = format!("{API}/calendars/{}/events", urlencode(&event.calendar_id));
    let created: CreatedEvent = serde_json::from_str(&call(
        token,
        "POST",
        &url,
        Some(event_body(event)?),
    )?)
    .context("parse created event")?;
    Ok(Event {
        id: created.id,
        change_key: created.etag,
        ..event.clone()
    })
}

/// Updates an event in place. PATCH rather than PUT: only the fields this
/// client owns are sent, so anything Google keeps that Meron does not model —
/// conferencing links, reminders, colours — survives the edit.
pub fn update_event(token: &str, event: &Event) -> Result<()> {
    let url = format!(
        "{API}/calendars/{}/events/{}",
        urlencode(&event.calendar_id),
        urlencode(&event.id),
    );
    call(token, "PATCH", &url, Some(event_body(event)?))?;
    Ok(())
}

pub fn delete_event(token: &str, calendar_id: &str, event_id: &str) -> Result<()> {
    let url = format!(
        "{API}/calendars/{}/events/{}",
        urlencode(calendar_id),
        urlencode(event_id),
    );
    // A delete of something already gone is what was asked for, not a failure.
    match call(token, "DELETE", &url, None) {
        Ok(_) => Ok(()),
        Err(err) if format!("{err:#}").contains("410") => Ok(()),
        Err(err) => Err(err),
    }
}

pub fn create_calendar(token: &str, name: &str) -> Result<String> {
    let url = format!("{API}/calendars");
    let created: CreatedCalendar = serde_json::from_str(&call(
        token,
        "POST",
        &url,
        Some(serde_json::json!({ "summary": name })),
    )?)
    .context("parse created calendar")?;
    Ok(created.id)
}

pub fn rename_calendar(token: &str, calendar_id: &str, name: &str) -> Result<()> {
    // The entry in *this account's* list is what carries the name it shows,
    // which is also the only name a shared calendar lets this account change.
    let url = format!("{API}/users/me/calendarList/{}", urlencode(calendar_id));
    call(
        token,
        "PATCH",
        &url,
        Some(serde_json::json!({ "summaryOverride": name })),
    )?;
    Ok(())
}

/// Removes a calendar: deleted if this account owns it, otherwise unsubscribed.
///
/// Google refuses to delete the primary calendar and any calendar this account
/// does not own; removing it from the list is what "remove" means for those,
/// and is what the other clients do.
pub fn delete_calendar(token: &str, calendar_id: &str, owned: bool) -> Result<()> {
    let url = if owned {
        format!("{API}/calendars/{}", urlencode(calendar_id))
    } else {
        format!("{API}/users/me/calendarList/{}", urlencode(calendar_id))
    };
    call(token, "DELETE", &url, None)?;
    Ok(())
}

// ---- Mapping ----------------------------------------------------------------

/// The request body for a create or update.
fn event_body(event: &Event) -> Result<serde_json::Value> {
    let (start, end) = if event.all_day {
        (
            serde_json::json!({ "date": date_only(event.start).context("start date")? }),
            serde_json::json!({ "date": date_only(event.end).context("end date")? }),
        )
    } else {
        (
            serde_json::json!({ "dateTime": rfc3339(event.start).context("start")? }),
            serde_json::json!({ "dateTime": rfc3339(event.end).context("end")? }),
        )
    };
    Ok(serde_json::json!({
        "summary": event.subject,
        "location": event.location.clone().unwrap_or_default(),
        "start": start,
        "end": end,
    }))
}

/// One API event as this codebase's own. `None` for an entry with no usable
/// instant: an event that cannot be placed on a calendar is not one.
fn to_event(source: GoogleEvent, calendar_id: &str) -> Option<Event> {
    let start_field = source.start.as_ref()?;
    let all_day = start_field.date.is_some();
    let start = instant(start_field)?;
    let end = source.end.as_ref().and_then(instant).unwrap_or(start);

    let organizer = source.organizer.as_ref().map(|person| Participant {
        name: person.display_name.clone().unwrap_or_default(),
        addr: person.email.clone().unwrap_or_default(),
        response: String::new(),
    });
    let my_response = source
        .attendees
        .iter()
        .find(|attendee| attendee.is_self)
        .and_then(|attendee| attendee.response_status.clone())
        .unwrap_or_default();
    let attendees = source
        .attendees
        .iter()
        .map(|attendee| Participant {
            name: attendee.display_name.clone().unwrap_or_default(),
            addr: attendee.email.clone().unwrap_or_default(),
            response: attendee.response_status.clone().unwrap_or_default(),
        })
        .collect();

    Some(Event {
        id: source.id,
        calendar_id: calendar_id.to_string(),
        change_key: source.etag,
        subject: source.summary.unwrap_or_default(),
        location: source.location,
        start,
        end,
        all_day,
        // With `singleEvents=true` an instance of a series carries the id of
        // the series it belongs to; a one-off carries none.
        is_recurring: source.recurring_event_id.is_some(),
        series_id: source.recurring_event_id.clone(),
        is_cancelled: source.status == "cancelled",
        free_busy: match source.transparency.as_deref() {
            Some("transparent") => "Free".to_string(),
            _ => "Busy".to_string(),
        },
        my_response,
        organizer,
        attendees,
    })
}

/// The instant a start or end refers to, as epoch seconds.
fn instant(value: &EventDateTime) -> Option<i64> {
    if let Some(text) = &value.date_time {
        return chrono::DateTime::parse_from_rfc3339(text)
            .ok()
            .map(|parsed| parsed.timestamp());
    }
    // An all-day event carries a calendar date and no zone. UTC midnight is
    // the only reading available here, and is what the subscribed-calendar
    // path already uses, so the two agree.
    let date = value.date.as_ref()?;
    chrono::NaiveDate::parse_from_str(date, "%Y-%m-%d")
        .ok()?
        .and_hms_opt(0, 0, 0)
        .map(|naive| naive.and_utc().timestamp())
}

fn rfc3339(epoch_seconds: i64) -> Option<String> {
    chrono::DateTime::from_timestamp(epoch_seconds, 0).map(|instant| instant.to_rfc3339())
}

fn date_only(epoch_seconds: i64) -> Option<String> {
    chrono::DateTime::from_timestamp(epoch_seconds, 0)
        .map(|instant| instant.format("%Y-%m-%d").to_string())
}

/// Percent-encodes a path or query component. Calendar ids are email-shaped
/// and event ids are opaque, so neither can be pasted into a URL raw.
fn urlencode(value: &str) -> String {
    let mut encoded = String::with_capacity(value.len());
    for byte in value.as_bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                encoded.push(*byte as char)
            }
            _ => encoded.push_str(&format!("%{byte:02X}")),
        }
    }
    encoded
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_timed_event_keeps_its_instant_and_its_people() {
        let source: GoogleEvent = serde_json::from_str(
            r#"{
              "id": "abc_20260901T080000Z",
              "etag": "\"123\"",
              "status": "confirmed",
              "summary": "Reunió equip",
              "location": "Sala 2",
              "start": {"dateTime": "2026-09-01T10:00:00+02:00"},
              "end": {"dateTime": "2026-09-01T11:30:00+02:00"},
              "recurringEventId": "abc",
              "organizer": {"email": "cap@example.org", "displayName": "La Cap"},
              "attendees": [
                {"email": "jo@example.org", "responseStatus": "accepted", "self": true},
                {"email": "altre@example.org", "responseStatus": "needsAction"}
              ]
            }"#,
        )
        .expect("the wire type should parse");
        let event = to_event(source, "cal").expect("a dated event maps");

        let expected = chrono::DateTime::parse_from_rfc3339("2026-09-01T08:00:00Z")
            .unwrap()
            .timestamp();
        assert_eq!(event.start, expected, "10:00 in +02:00 is 08:00 UTC");
        assert_eq!(event.end - event.start, 90 * 60);
        assert!(!event.all_day);
        assert!(event.is_recurring, "an instance names the series it came from");
        assert_eq!(event.my_response, "accepted", "this mailbox's own answer");
        assert_eq!(event.attendees.len(), 2);
        assert_eq!(event.organizer.as_ref().unwrap().name, "La Cap");
        assert_eq!(event.free_busy, "Busy");
    }

    #[test]
    fn an_all_day_event_is_a_date_not_an_instant() {
        let source: GoogleEvent = serde_json::from_str(
            r#"{"id":"x","status":"confirmed","summary":"Festiu",
                "start":{"date":"2026-09-11"},"end":{"date":"2026-09-12"}}"#,
        )
        .expect("parse");
        let event = to_event(source, "cal").expect("map");
        assert!(event.all_day);
        assert_eq!(event.end - event.start, 24 * 3600);
        assert!(!event.is_recurring, "a one-off names no series");
    }

    #[test]
    fn an_entry_with_no_start_cannot_be_placed_on_a_calendar() {
        let source: GoogleEvent =
            serde_json::from_str(r#"{"id":"x","status":"confirmed","summary":"Sense data"}"#)
                .expect("parse");
        assert!(to_event(source, "cal").is_none());
    }

    #[test]
    fn a_calendar_someone_else_owns_is_read_only() {
        let list: CalendarList = serde_json::from_str(
            r##"{"items":[
                 {"id":"me@gmail.com","summary":"Personal","primary":true,"accessRole":"owner",
                  "backgroundColor":"#9fe1e7"},
                 {"id":"team@group.calendar.google.com","summary":"Equip","accessRole":"reader"},
                 {"id":"gone@x","summary":"Fora","accessRole":"owner","deleted":true}
               ]}"##,
        )
        .expect("parse");
        // Exercised through the same mapping the request path uses.
        let calendars: Vec<Calendar> = list
            .items
            .into_iter()
            .filter(|entry| !entry.deleted)
            .map(|entry| Calendar {
                id: entry.id,
                name: entry.summary,
                is_default: entry.primary,
                read_only: !matches!(entry.access_role.as_str(), "owner" | "writer"),
                color: entry.background_color,
                ..Default::default()
            })
            .collect();

        assert_eq!(calendars.len(), 2, "a calendar removed from the list is gone");
        assert!(calendars[0].is_default && !calendars[0].read_only);
        assert_eq!(
            calendars[0].color.as_deref(),
            Some("#9fe1e7"),
            "the colour Google draws it in comes along"
        );
        assert!(calendars[1].read_only, "reader access cannot be edited");
    }

    #[test]
    fn ids_are_escaped_before_they_reach_a_url() {
        // Calendar ids are email-shaped and event ids can carry anything.
        assert_eq!(urlencode("me@gmail.com"), "me%40gmail.com");
        assert_eq!(urlencode("a b/c"), "a%20b%2Fc");
        assert_eq!(urlencode("plain-id_1.2~3"), "plain-id_1.2~3");
    }

    #[test]
    fn a_token_without_calendar_permission_says_what_to_do_about_it() {
        // The account works and its calendar does not; nothing the user can
        // see explains that, so the message names the fix rather than the
        // protocol.
        let body = r#"{"error":{"code":403,"message":"Request had insufficient authentication scopes."}}"#;
        let told = explain(403, body);
        assert!(told.contains("reconnect"), "got: {told}");
        assert!(!told.contains("403"), "the status code is not the point");

        // Anything else is reported as what it was.
        let other = explain(404, r#"{"error":{"code":404,"message":"Not Found"}}"#);
        assert!(other.contains("404") && other.contains("Not Found"));
    }

    #[test]
    fn an_error_body_is_reported_by_its_sentence() {
        let body = r#"{"error":{"code":403,"message":"Request had insufficient authentication scopes."}}"#;
        assert_eq!(brief(body), "Request had insufficient authentication scopes.");
        // A body that is not the usual envelope still says something.
        assert!(brief("<html>502</html>").contains("502"));
    }
}

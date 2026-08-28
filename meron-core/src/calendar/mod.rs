//! Calendar model shared by every provider.
//!
//! One [`Event`] is one *occurrence*, never a series: providers expand
//! recurrences over a requested date range, so a recurring meeting arrives as
//! a discrete event per instance and this client never interprets recurrence
//! rules — nor the daylight-saving shifts that make hand-rolled expansion so
//! error-prone.
//!
//! The provider backends (Exchange today, Google next) map their own answers
//! onto these types; everything above this line is provider-neutral.

pub mod google;
pub mod route;
pub mod subscription;

use anyhow::Result;
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};

/// Where a calendar comes from.
///
/// This decides how it syncs and whether it can be written to, which is the
/// distinction a reader actually reasons about when asking "why can I not edit
/// this?" — so it is modelled rather than inferred.
#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum CalendarKind {
    /// Lives on the server of a mail account, and syncs with it.
    #[default]
    Account,
    /// Lives only in this copy of Meron. Nothing syncs it and nothing else has
    /// a copy — which the interface has to say plainly.
    Local,
    /// A calendar file fetched from a URL. Read-only: it belongs to whoever
    /// publishes it.
    Subscribed,
}

impl CalendarKind {
    pub fn as_str(self) -> &'static str {
        match self {
            CalendarKind::Account => "account",
            CalendarKind::Local => "local",
            CalendarKind::Subscribed => "subscribed",
        }
    }

    fn parse(value: &str) -> Self {
        match value {
            "local" => CalendarKind::Local,
            "subscribed" => CalendarKind::Subscribed,
            _ => CalendarKind::Account,
        }
    }
}

/// A calendar, wherever it comes from.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct Calendar {
    /// The provider's own identifier for this calendar.
    pub id: String,
    pub name: String,
    /// The account's primary calendar, which is where a new event goes unless
    /// the user picks another.
    #[serde(default)]
    pub is_default: bool,
    /// Whether the user wants it shown. Hiding a calendar never forgets it.
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default)]
    pub color: Option<String>,
    #[serde(default)]
    pub kind: CalendarKind,
    /// Where a subscribed calendar is fetched from.
    #[serde(default)]
    pub url: Option<String>,
    /// Whether events on it can be changed. Always true for a subscription:
    /// the file belongs to whoever publishes it.
    #[serde(default)]
    pub read_only: bool,
    /// When a subscription was last fetched, epoch seconds.
    #[serde(default)]
    pub synced_at: i64,
}

fn default_true() -> bool {
    true
}

/// One person on an event, and how they answered.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct Participant {
    #[serde(default)]
    pub name: String,
    /// An email address. Servers that answer with an internal directory
    /// identifier have it resolved before it reaches here — see
    /// [`crate::exchange`] — because an unresolved one is unreadable.
    #[serde(default)]
    pub addr: String,
    /// "accept", "decline", "tentative", "none", or empty when not applicable.
    #[serde(default)]
    pub response: String,
}

/// One occurrence of an event.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct Event {
    pub id: String,
    pub calendar_id: String,
    /// The provider's version stamp, required by writes that must not clobber
    /// a change made elsewhere.
    #[serde(default)]
    pub change_key: Option<String>,
    pub subject: String,
    #[serde(default)]
    pub location: Option<String>,
    /// Epoch seconds. Always an instant, even for all-day events: the server
    /// resolves those against the calendar's timezone, which is what keeps a
    /// series correct across a daylight-saving boundary.
    pub start: i64,
    pub end: i64,
    #[serde(default)]
    pub all_day: bool,
    /// Whether this occurrence belongs to a series. Purely informational —
    /// the occurrence is complete on its own.
    #[serde(default)]
    pub is_recurring: bool,
    /// How often it repeats, when it is being created as a series. Never read
    /// back from a server: what comes back are the occurrences themselves.
    #[serde(default)]
    pub recurrence: Option<Recurrence>,
    /// How many minutes before the start a reminder is due, when one is set.
    /// `None` means no reminder — which is not the same as one set to zero.
    #[serde(default)]
    pub reminder_minutes: Option<i64>,
    /// The event's own notes, as plain text. Servers keep this as HTML more
    /// often than not; it is converted on the way in, since a calendar shows
    /// notes rather than renders documents.
    #[serde(default)]
    pub description: String,
    /// The series this occurrence came from, when the server names one.
    ///
    /// Every occurrence of one series shares it — the iCalendar UID on
    /// Exchange and in published files, the master's id on Google — which is
    /// what lets occurrences be grouped back into a series without this client
    /// interpreting a recurrence rule. `None` when the server does not say.
    #[serde(default)]
    pub series_id: Option<String>,
    #[serde(default)]
    pub is_cancelled: bool,
    /// How the time shows on the organizer's calendar: "free", "tentative",
    /// "busy", "oof", "workingelsewhere", or empty.
    #[serde(default)]
    pub free_busy: String,
    /// This mailbox's own answer to the invitation.
    #[serde(default)]
    pub my_response: String,
    #[serde(default)]
    pub organizer: Option<Participant>,
    #[serde(default)]
    pub attendees: Vec<Participant>,
}

/// How often an event repeats, as asked for when creating one.
///
/// Write-only, and deliberately not stored: the server keeps the rule and
/// hands back the occurrences it expands from it, which is the only form this
/// client ever reads. Keeping a copy here would be a second truth to go stale.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct Recurrence {
    pub freq: Frequency,
    /// Every N days/weeks/months/years. One means every one.
    #[serde(default = "one")]
    pub interval: u16,
    /// Which days a weekly rule falls on, as 0 = Monday … 6 = Sunday. Empty
    /// means the day the event itself starts on.
    #[serde(default)]
    pub weekdays: Vec<u8>,
    /// The last day the series may fall on, as epoch seconds.
    #[serde(default)]
    pub until: Option<i64>,
    /// Or a fixed number of occurrences. `until` and `count` are alternatives;
    /// when both are given, `until` wins, because a date is what the reader
    /// picked from a calendar.
    #[serde(default)]
    pub count: Option<u16>,
}

fn one() -> u16 {
    1
}

#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Frequency {
    #[default]
    Daily,
    Weekly,
    Monthly,
    Yearly,
}

impl Recurrence {
    /// The English weekday names EWS expects, in the order this codebase uses.
    pub const EWS_DAYS: [&'static str; 7] = [
        "Monday",
        "Tuesday",
        "Wednesday",
        "Thursday",
        "Friday",
        "Saturday",
        "Sunday",
    ];

    /// The two-letter codes an RRULE uses, in the same order.
    pub const RRULE_DAYS: [&'static str; 7] = ["MO", "TU", "WE", "TH", "FR", "SA", "SU"];

    /// The days a weekly rule falls on, defaulting to the day the event starts
    /// when the caller named none.
    pub fn days_or_start(&self, start: i64) -> Vec<u8> {
        if !self.weekdays.is_empty() {
            let mut days: Vec<u8> = self.weekdays.iter().copied().filter(|d| *d < 7).collect();
            days.sort_unstable();
            days.dedup();
            if !days.is_empty() {
                return days;
            }
        }
        let weekday = chrono::DateTime::from_timestamp(start, 0)
            .map(|instant| {
                chrono::Datelike::weekday(&instant).num_days_from_monday() as u8
            })
            .unwrap_or(0);
        vec![weekday]
    }
}

/// An answer to a meeting invitation.
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Response {
    Accept,
    Tentative,
    Decline,
}

impl Response {
    pub fn parse(value: &str) -> Option<Self> {
        match value.to_ascii_lowercase().as_str() {
            "accept" | "accepted" => Some(Response::Accept),
            "tentative" | "tentatively" => Some(Response::Tentative),
            "decline" | "declined" => Some(Response::Decline),
            _ => None,
        }
    }

    /// What Google calls it.
    pub fn google_status(self) -> &'static str {
        match self {
            Response::Accept => "accepted",
            Response::Tentative => "tentative",
            Response::Decline => "declined",
        }
    }
}

/// An event's notes as plain text, whatever the server sent.
///
/// Exchange declares the type of its body, but Google and published `.ics`
/// files hand over a string with no type at all — and both routinely carry
/// HTML, since invitations are so often generated by mail systems. Markup
/// shown raw is worse than useless, so anything that looks like HTML is
/// converted with the same reader the mail side uses.
pub fn plain_notes(raw: &str) -> String {
    if looks_like_html(raw) {
        crate::parse::html_to_text(raw).trim().to_string()
    } else {
        raw.trim().to_string()
    }
}

/// Whether a string carries markup, as opposed to mentioning a `<` in passing.
/// A tag is an angle bracket, a name, and a closing bracket somewhere after.
fn looks_like_html(text: &str) -> bool {
    let mut chars = text.char_indices();
    while let Some((index, char)) = chars.next() {
        if char != '<' {
            continue;
        }
        let rest = &text[index + 1..];
        let name_start = rest.strip_prefix('/').unwrap_or(rest);
        if !name_start.starts_with(|c: char| c.is_ascii_alphabetic()) {
            continue;
        }
        // A tag closes on the same line; a stray `<` in prose usually does not.
        if let Some(close) = name_start.find('>') {
            if !name_start[..close].contains('\n') {
                return true;
            }
        }
    }
    false
}

// ---- Store ------------------------------------------------------------------

pub fn upsert_calendars(conn: &Connection, account: &str, calendars: &[Calendar]) -> Result<()> {
    let tx = conn.unchecked_transaction()?;
    for calendar in calendars {
        upsert_one(&tx, account, calendar)?;
    }
    tx.commit()?;
    Ok(())
}

/// Records a server's complete listing of an account's calendars.
///
/// A listing is the whole truth about what that account's server offers, so a
/// calendar absent from it is one that no longer exists — removed here, or
/// from another client. Keeping its row would show the user a calendar they
/// cannot open and cannot get rid of, so the row goes with it. Local
/// calendars and subscriptions are not the server's to report and are left
/// untouched.
pub fn replace_account_calendars(
    conn: &Connection,
    account: &str,
    calendars: &[Calendar],
) -> Result<()> {
    let tx = conn.unchecked_transaction()?;
    for calendar in calendars {
        upsert_one(&tx, account, calendar)?;
    }

    // An empty listing is not taken as "the account has no calendars": a
    // mailbox always keeps at least its default one, so an empty answer is a
    // fault rather than a fact, and acting on it would delete everything the
    // user has. Nothing is pruned until the server says something.
    if !calendars.is_empty() {
        let offered: std::collections::HashSet<&str> =
            calendars.iter().map(|calendar| calendar.id.as_str()).collect();
        let stale: Vec<String> = {
            let mut stmt = tx.prepare(
                "SELECT provider_id FROM calendars WHERE account = ?1 AND kind = 'account'",
            )?;
            let rows = stmt.query_map(params![account], |row| row.get::<_, String>(0))?;
            rows.filter_map(|id| id.ok())
                .filter(|id| !offered.contains(id.as_str()))
                .collect()
        };
        for id in stale {
            tx.execute(
                "DELETE FROM calendar_events WHERE account = ?1 AND calendar_id = ?2",
                params![account, id],
            )?;
            tx.execute(
                "DELETE FROM calendars WHERE account = ?1 AND provider_id = ?2",
                params![account, id],
            )?;
        }
    }
    tx.commit()?;
    Ok(())
}

/// One calendar's row, inside a caller's transaction.
fn upsert_one(tx: &rusqlite::Transaction<'_>, account: &str, calendar: &Calendar) -> Result<()> {
    // `enabled` is deliberately not overwritten: it is the user's choice, and
    // a resync — which arrives with no opinion on it — must not undo it.
    //
    // `color` is the user's choice too, but only once they have made one:
    // until then the server's own colour is worth adopting, since a reader who
    // has told Google that work is green should not have to say it again here.
    // COALESCE is what keeps a chosen colour and fills in an unchosen one.
    tx.execute(
        "INSERT INTO calendars(
           account, provider_id, name, is_default, color, kind, url, read_only)
         VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
         ON CONFLICT(account, provider_id) DO UPDATE SET
           name = excluded.name,
           is_default = excluded.is_default,
           kind = excluded.kind,
           url = excluded.url,
           read_only = excluded.read_only,
           color = COALESCE(calendars.color, excluded.color)",
        params![
            account,
            calendar.id,
            calendar.name,
            calendar.is_default as i64,
            calendar.color,
            calendar.kind.as_str(),
            calendar.url,
            calendar.read_only as i64
        ],
    )?;
    Ok(())
}

pub fn get_calendars(conn: &Connection, account: &str) -> Result<Vec<Calendar>> {
    let mut stmt = conn.prepare(
        "SELECT provider_id, name, is_default, enabled, color, kind, url, read_only,
                synced_at
         FROM calendars
         WHERE account = ?1 ORDER BY is_default DESC, name",
    )?;
    let calendars = stmt
        .query_map(params![account], |row| {
            Ok(Calendar {
                id: row.get(0)?,
                name: row.get(1)?,
                is_default: row.get::<_, i64>(2)? != 0,
                enabled: row.get::<_, i64>(3)? != 0,
                color: row.get(4)?,
                kind: CalendarKind::parse(&row.get::<_, String>(5)?),
                url: row.get(6)?,
                read_only: row.get::<_, i64>(7)? != 0,
                synced_at: row.get(8)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(calendars)
}

pub fn set_calendar_enabled(
    conn: &Connection,
    account: &str,
    calendar_id: &str,
    enabled: bool,
) -> Result<()> {
    conn.execute(
        "UPDATE calendars SET enabled = ?3 WHERE account = ?1 AND provider_id = ?2",
        params![account, calendar_id, enabled as i64],
    )?;
    Ok(())
}

/// Replaces a calendar's cached occurrences for one window.
///
/// A window's rows are a snapshot, not an accumulation: because the server
/// expands recurrences per request, the only faithful cache of "what is on
/// this calendar between these dates" is whatever the last answer said.
/// Replacing is also what makes a cancelled or moved occurrence disappear
/// without needing tombstones.
pub fn replace_window(
    conn: &Connection,
    account: &str,
    calendar_id: &str,
    window: (i64, i64),
    events: &[Event],
) -> Result<()> {
    let (from, to) = window;
    let tx = conn.unchecked_transaction()?;
    // Overlap, not containment: an event starting before the window and
    // ending inside it belongs to this window's answer too.
    tx.execute(
        "DELETE FROM calendar_events
         WHERE account = ?1 AND calendar_id = ?2 AND end_utc > ?3 AND start_utc < ?4",
        params![account, calendar_id, from, to],
    )?;
    for event in events {
        tx.execute(
            "INSERT OR REPLACE INTO calendar_events(
               account, calendar_id, event_id, change_key, subject, location,
               start_utc, end_utc, all_day, is_recurring, is_cancelled,
               free_busy, my_response, organizer, attendees, series_id,
               description, reminder_minutes)
             VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16,
                    ?17, ?18)",
            params![
                account,
                calendar_id,
                event.id,
                event.change_key,
                event.subject,
                event.location,
                event.start,
                event.end,
                event.all_day as i64,
                event.is_recurring as i64,
                event.is_cancelled as i64,
                event.free_busy,
                event.my_response,
                event
                    .organizer
                    .as_ref()
                    .map(serde_json::to_string)
                    .transpose()?,
                serde_json::to_string(&event.attendees)?,
                event.series_id,
                event.description,
                event.reminder_minutes,
            ],
        )?;
    }
    tx.commit()?;
    Ok(())
}

/// Cached occurrences overlapping a window, across every enabled calendar of
/// an account, earliest first.
pub fn events_in_window(
    conn: &Connection,
    account: &str,
    window: (i64, i64),
) -> Result<Vec<Event>> {
    let (from, to) = window;
    let mut stmt = conn.prepare(
        "SELECT e.calendar_id, e.event_id, e.change_key, e.subject, e.location,
                e.start_utc, e.end_utc, e.all_day, e.is_recurring, e.is_cancelled,
                e.free_busy, e.my_response, e.organizer, e.attendees, e.series_id,
                e.description, e.reminder_minutes
         FROM calendar_events e
         JOIN calendars c
           ON c.account = e.account AND c.provider_id = e.calendar_id
         WHERE e.account = ?1 AND c.enabled = 1
           AND e.end_utc > ?2 AND e.start_utc < ?3
         ORDER BY e.start_utc, e.subject",
    )?;
    let events = stmt
        .query_map(params![account, from, to], |row| {
            Ok(Event {
                calendar_id: row.get(0)?,
                id: row.get(1)?,
                change_key: row.get(2)?,
                subject: row.get(3)?,
                location: row.get(4)?,
                start: row.get(5)?,
                end: row.get(6)?,
                all_day: row.get::<_, i64>(7)? != 0,
                is_recurring: row.get::<_, i64>(8)? != 0,
                is_cancelled: row.get::<_, i64>(9)? != 0,
                free_busy: row.get::<_, Option<String>>(10)?.unwrap_or_default(),
                my_response: row.get::<_, Option<String>>(11)?.unwrap_or_default(),
                organizer: row
                    .get::<_, Option<String>>(12)?
                    .and_then(|json| serde_json::from_str(&json).ok()),
                attendees: row
                    .get::<_, Option<String>>(13)?
                    .and_then(|json| serde_json::from_str(&json).ok())
                    .unwrap_or_default(),
                series_id: row.get(14)?,
                description: row.get::<_, Option<String>>(15)?.unwrap_or_default(),
                reminder_minutes: row.get(16)?,
                recurrence: None,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(events)
}

/// Records that a calendar's contents were read from its server just now.
///
/// The column existed and was never written, so every calendar claimed to have
/// last synced at the epoch. A time nobody sets is worse than no time at all:
/// it reads as an answer.
pub fn mark_calendar_synced(
    conn: &Connection,
    account: &str,
    calendar_id: &str,
    now: i64,
) -> Result<()> {
    conn.execute(
        "UPDATE calendars SET synced_at = ?3 WHERE account = ?1 AND provider_id = ?2",
        params![account, calendar_id, now],
    )?;
    Ok(())
}

/// Drops every cached occurrence of a series.
///
/// A series deleted on the server leaves nothing behind, so neither should the
/// cache: forgetting only the occurrence the reader clicked would leave the
/// rest on screen until the next sync, looking like a delete that half worked.
pub fn forget_series(conn: &Connection, account: &str, series_id: &str) -> Result<usize> {
    Ok(conn.execute(
        "DELETE FROM calendar_events WHERE account = ?1 AND series_id = ?2",
        params![account, series_id],
    )?)
}

/// Reminders that have come due and have not been raised yet.
///
/// Only for events still ahead: a reminder for something already started has
/// missed its moment, and raising it late would be worse than silence. The
/// caller marks each as raised, so one reminder is given once even though the
/// window it lives in is resynced repeatedly.
pub fn due_reminders(conn: &Connection, account: &str, now: i64) -> Result<Vec<Event>> {
    let mut stmt = conn.prepare(
        "SELECT e.calendar_id, e.event_id, e.subject, e.start_utc, e.end_utc,
                e.all_day, e.location, e.reminder_minutes
         FROM calendar_events e
         JOIN calendars c
           ON c.account = e.account AND c.provider_id = e.calendar_id
         WHERE e.account = ?1 AND c.enabled = 1
           AND e.reminder_minutes IS NOT NULL
           AND e.is_cancelled = 0
           AND e.start_utc > ?2
           AND e.start_utc - (e.reminder_minutes * 60) <= ?2
           AND NOT EXISTS (
                 SELECT 1 FROM calendar_reminders_fired f
                 WHERE f.account = e.account AND f.event_id = e.event_id)
         ORDER BY e.start_utc",
    )?;
    let events = stmt
        .query_map(params![account, now], |row| {
            Ok(Event {
                calendar_id: row.get(0)?,
                id: row.get(1)?,
                subject: row.get(2)?,
                start: row.get(3)?,
                end: row.get(4)?,
                all_day: row.get::<_, i64>(5)? != 0,
                location: row.get(6)?,
                reminder_minutes: row.get(7)?,
                ..Default::default()
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(events)
}

/// Records that a reminder has been raised, so it is not raised again.
pub fn mark_reminder_fired(conn: &Connection, account: &str, event_id: &str, now: i64) -> Result<()> {
    conn.execute(
        "INSERT OR REPLACE INTO calendar_reminders_fired(account, event_id, fired_at)
         VALUES(?1, ?2, ?3)",
        params![account, event_id, now],
    )?;
    Ok(())
}

/// Drops records of reminders long past, so the table does not grow forever.
/// A fortnight is well beyond any window a reminder could still be pending in.
pub fn forget_old_reminders(conn: &Connection, now: i64) -> Result<()> {
    conn.execute(
        "DELETE FROM calendar_reminders_fired WHERE fired_at < ?1",
        params![now - 14 * 24 * 3600],
    )?;
    Ok(())
}

/// The colour a calendar is drawn with, which is a local choice.
///
/// Exchange has no calendar colour other clients agree on, so syncing one
/// would either fail or write a property nothing else reads. Keeping it local
/// is predictable: your colours, in your copy.
pub fn set_calendar_color(
    conn: &Connection,
    account: &str,
    calendar_id: &str,
    color: Option<&str>,
) -> Result<()> {
    conn.execute(
        "UPDATE calendars SET color = ?3 WHERE account = ?1 AND provider_id = ?2",
        params![account, calendar_id, color],
    )?;
    Ok(())
}

/// Renames a calendar locally, after the server accepted the rename.
pub fn rename_calendar(
    conn: &Connection,
    account: &str,
    calendar_id: &str,
    name: &str,
) -> Result<()> {
    conn.execute(
        "UPDATE calendars SET name = ?3 WHERE account = ?1 AND provider_id = ?2",
        params![account, calendar_id, name],
    )?;
    Ok(())
}

/// Drops a calendar and its cached events, after the server deleted it.
pub fn forget_calendar(conn: &Connection, account: &str, calendar_id: &str) -> Result<()> {
    let tx = conn.unchecked_transaction()?;
    tx.execute(
        "DELETE FROM calendar_events WHERE account = ?1 AND calendar_id = ?2",
        params![account, calendar_id],
    )?;
    tx.execute(
        "DELETE FROM calendars WHERE account = ?1 AND provider_id = ?2",
        params![account, calendar_id],
    )?;
    tx.commit()?;
    Ok(())
}

/// Drops one cached occurrence, after the server accepted its deletion.
pub fn forget_event(conn: &Connection, account: &str, event_id: &str) -> Result<()> {
    conn.execute(
        "DELETE FROM calendar_events WHERE account = ?1 AND event_id = ?2",
        params![account, event_id],
    )?;
    Ok(())
}

/// Drops everything an account cached, for account removal.
pub fn delete_account_calendars(conn: &Connection, account: &str) -> Result<()> {
    conn.execute(
        "DELETE FROM calendar_events WHERE account = ?1",
        params![account],
    )?;
    conn.execute("DELETE FROM calendars WHERE account = ?1", params![account])?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store() -> Connection {
        crate::store::open_in_memory_for_test().expect("open store")
    }

    fn event(id: &str, start: i64, end: i64) -> Event {
        Event {
            id: id.to_string(),
            calendar_id: "cal".to_string(),
            subject: format!("event {id}"),
            start,
            end,
            ..Default::default()
        }
    }

    fn a_calendar() -> Calendar {
        Calendar {
            id: "cal".to_string(),
            name: "Calendario".to_string(),
            is_default: true,
            enabled: true,
            ..Default::default()
        }
    }

    #[test]
    fn hiding_a_calendar_survives_a_resync_of_the_list() {
        let conn = store();
        upsert_calendars(&conn, "acct", &[a_calendar()]).unwrap();
        set_calendar_enabled(&conn, "acct", "cal", false).unwrap();

        // The server does not know the user hid it, so a refreshed listing
        // arrives enabled; re-showing it here would undo their choice on every
        // sync.
        let renamed = Calendar {
            name: "Calendario del trabajo".to_string(),
            ..a_calendar()
        };
        upsert_calendars(&conn, "acct", &[renamed]).unwrap();

        let stored = get_calendars(&conn, "acct").unwrap();
        assert_eq!(stored.len(), 1);
        assert_eq!(stored[0].name, "Calendario del trabajo", "the name refreshes");
        assert!(!stored[0].enabled, "the user's choice does not");
    }

    /// A real invitation body, generated by a mail system: a layout table with
    /// inline styles wrapped around one sentence.
    const HTML_NOTES: &str = "<table bgcolor=\"#EFEFEF\" border=\"0\" width=\"100%\"><tbody><tr>\
<td align=\"left\" class=\"ph52\" style=\"font-size:16px;color:#494950\">Solicitud:</td></tr>\
<tr><td style=\"padding:0 40px 20px;\">Revisió del vehicle</td></tr></tbody></table>";

    #[test]
    fn a_reminder_comes_due_once_and_only_while_the_event_is_still_ahead() {
        let conn = store();
        upsert_calendars(&conn, "acct", &[a_calendar()]).unwrap();
        let now = 1_000_000i64;
        let with_reminder = |id: &str, start: i64, minutes: Option<i64>| Event {
            reminder_minutes: minutes,
            ..event(id, start, start + 3600)
        };
        replace_window(
            &conn,
            "acct",
            "cal",
            (0, now + 100_000),
            &[
                // Due: starts in ten minutes, reminder set for fifteen.
                with_reminder("soon", now + 600, Some(15)),
                // Not yet: an hour off, reminder set for ten minutes.
                with_reminder("later", now + 3600, Some(10)),
                // Missed its moment: already started.
                with_reminder("started", now - 60, Some(15)),
                // No reminder asked for.
                with_reminder("silent", now + 600, None),
            ],
        )
        .unwrap();

        let due = due_reminders(&conn, "acct", now).unwrap();
        assert_eq!(due.len(), 1, "only the one whose moment has come");
        assert_eq!(due[0].id, "soon");

        // Raised once: the same query must not offer it again.
        mark_reminder_fired(&conn, "acct", "soon", now).unwrap();
        assert!(due_reminders(&conn, "acct", now).unwrap().is_empty());

        // And a resync, which replaces the window's rows wholesale, must not
        // bring it back — that is why the record lives in its own table.
        replace_window(
            &conn,
            "acct",
            "cal",
            (0, now + 100_000),
            &[with_reminder("soon", now + 600, Some(15))],
        )
        .unwrap();
        assert!(
            due_reminders(&conn, "acct", now).unwrap().is_empty(),
            "a reminder already given is not given again after a sync"
        );
    }

    #[test]
    fn a_hidden_calendars_reminders_stay_quiet() {
        let conn = store();
        upsert_calendars(&conn, "acct", &[a_calendar()]).unwrap();
        let now = 1_000_000i64;
        replace_window(
            &conn,
            "acct",
            "cal",
            (0, now + 100_000),
            &[Event {
                reminder_minutes: Some(15),
                ..event("soon", now + 600, now + 4200)
            }],
        )
        .unwrap();
        assert_eq!(due_reminders(&conn, "acct", now).unwrap().len(), 1);

        // Hiding a calendar means not hearing from it either.
        set_calendar_enabled(&conn, "acct", "cal", false).unwrap();
        assert!(due_reminders(&conn, "acct", now).unwrap().is_empty());
    }

    #[test]
    fn notes_arrive_as_text_even_when_the_server_sends_markup() {
        let notes = plain_notes(HTML_NOTES);
        assert!(notes.contains("Solicitud:"), "the words survive: {notes}");
        assert!(notes.contains("Revisió del vehicle"));
        assert!(!notes.contains("<td"), "the markup does not: {notes}");
        assert!(!notes.contains("bgcolor"));
        assert!(!notes.contains("#EFEFEF"), "nor the styling");
    }

    #[test]
    fn plain_notes_are_left_alone() {
        // Prose that merely mentions an angle bracket is not markup, and
        // running it through an HTML reader would mangle it.
        let plain = "Portar el portàtil.\nCosta < 20 € el pàrquing.";
        assert_eq!(plain_notes(plain), plain);
        assert_eq!(plain_notes("  Sala 2  "), "Sala 2", "and edges are trimmed");
    }

    #[test]
    fn occurrences_of_one_series_can_be_found_by_the_series_they_came_from() {
        let conn = store();
        upsert_calendars(&conn, "acct", &[a_calendar()]).unwrap();
        let day = 24 * 3600;
        let occurrence = |id: &str, start: i64, series: Option<&str>| Event {
            series_id: series.map(str::to_string),
            ..event(id, start, start + 3600)
        };
        replace_window(
            &conn,
            "acct",
            "cal",
            (0, 30 * day),
            &[
                occurrence("a", day, Some("standup@example.org")),
                occurrence("b", 8 * day, Some("standup@example.org")),
                occurrence("c", 3 * day, None),
            ],
        )
        .unwrap();

        let stored = events_in_window(&conn, "acct", (0, 30 * day)).unwrap();
        let series: Vec<_> = stored
            .iter()
            .filter(|e| e.series_id.as_deref() == Some("standup@example.org"))
            .collect();
        assert_eq!(series.len(), 2, "both occurrences name the series they came from");
        assert!(
            stored.iter().any(|e| e.id == "c" && e.series_id.is_none()),
            "a one-off names no series"
        );
    }

    #[test]
    fn the_servers_colour_fills_in_until_the_user_picks_one() {
        let conn = store();
        let from_google = Calendar {
            color: Some("#9fe1e7".to_string()),
            ..a_calendar()
        };
        upsert_calendars(&conn, "acct", &[from_google.clone()]).unwrap();
        assert_eq!(
            get_calendars(&conn, "acct").unwrap()[0].color.as_deref(),
            Some("#9fe1e7"),
            "with no choice of the reader's, the server's colour is adopted"
        );

        set_calendar_color(&conn, "acct", "cal", Some("#E24C3B")).unwrap();
        upsert_calendars(&conn, "acct", &[from_google]).unwrap();
        assert_eq!(
            get_calendars(&conn, "acct").unwrap()[0].color.as_deref(),
            Some("#E24C3B"),
            "once chosen here, a resync does not take it back"
        );
    }

    #[test]
    fn a_calendar_records_when_it_was_last_read_from_its_server() {
        let conn = store();
        upsert_calendars(&conn, "acct", &[a_calendar()]).unwrap();
        assert_eq!(
            get_calendars(&conn, "acct").unwrap()[0].synced_at,
            0,
            "never synced is the epoch, and reads as never"
        );

        mark_calendar_synced(&conn, "acct", "cal", 1_700_000_000).unwrap();
        assert_eq!(get_calendars(&conn, "acct").unwrap()[0].synced_at, 1_700_000_000);

        // And a later listing does not reset it: the name may change, the fact
        // that it was read does not un-happen.
        upsert_calendars(&conn, "acct", &[a_calendar()]).unwrap();
        assert_eq!(get_calendars(&conn, "acct").unwrap()[0].synced_at, 1_700_000_000);
    }

    #[test]
    fn a_calendar_the_server_no_longer_offers_is_dropped() {
        let conn = store();
        let extra = Calendar {
            id: "cal2".to_string(),
            name: "Cumpleaños".to_string(),
            is_default: false,
            enabled: true,
            ..Default::default()
        };
        replace_account_calendars(&conn, "acct", &[a_calendar(), extra]).unwrap();
        let on_cal2 = Event {
            calendar_id: "cal2".to_string(),
            ..event("e1", 10, 20)
        };
        replace_window(&conn, "acct", "cal2", (0, 100), &[on_cal2]).unwrap();

        // Removed here or from another client: the next listing simply does
        // not mention it. Keeping the row would show a calendar that cannot be
        // opened and cannot be removed.
        replace_account_calendars(&conn, "acct", &[a_calendar()]).unwrap();

        let stored = get_calendars(&conn, "acct").unwrap();
        assert_eq!(stored.len(), 1);
        assert_eq!(stored[0].id, "cal");
        let events = events_in_window(&conn, "acct", (0, 100)).unwrap();
        assert!(events.is_empty(), "its events go with it");
    }

    #[test]
    fn a_listing_only_speaks_for_the_account_and_only_when_it_says_something() {
        let conn = store();
        let local = Calendar {
            id: "local:1".to_string(),
            name: "Personal".to_string(),
            kind: CalendarKind::Local,
            enabled: true,
            ..Default::default()
        };
        upsert_calendars(&conn, "acct", &[local]).unwrap();
        replace_account_calendars(&conn, "acct", &[a_calendar()]).unwrap();
        assert_eq!(
            get_calendars(&conn, "acct").unwrap().len(),
            2,
            "a local calendar is not the server's to report, and survives its listing"
        );

        // An empty answer from a mailbox that always keeps a default calendar
        // is a fault, not a fact; acting on it would delete everything.
        replace_account_calendars(&conn, "acct", &[]).unwrap();
        assert_eq!(
            get_calendars(&conn, "acct").unwrap().len(),
            2,
            "nothing is pruned until the server says something"
        );
    }

    #[test]
    fn a_calendar_remembers_where_it_came_from() {
        let conn = store();
        upsert_calendars(
            &conn,
            "acct",
            &[
                a_calendar(),
                Calendar {
                    id: "local:1".to_string(),
                    name: "Personal".to_string(),
                    kind: CalendarKind::Local,
                    enabled: true,
                    ..Default::default()
                },
                Calendar {
                    id: "sub:1".to_string(),
                    name: "Festivos".to_string(),
                    kind: CalendarKind::Subscribed,
                    url: Some("https://example.org/holidays.ics".to_string()),
                    read_only: true,
                    enabled: true,
                    ..Default::default()
                },
            ],
        )
        .unwrap();

        let stored = get_calendars(&conn, "acct").unwrap();
        let by_id = |id: &str| stored.iter().find(|c| c.id == id).cloned().unwrap();

        assert_eq!(by_id("cal").kind, CalendarKind::Account);
        assert_eq!(by_id("local:1").kind, CalendarKind::Local);

        // A subscription keeps where it is fetched from, and that it cannot be
        // written to: without the URL there is nothing to refresh, and without
        // the flag the interface would offer edits the publisher would never
        // receive.
        let subscribed = by_id("sub:1");
        assert_eq!(subscribed.kind, CalendarKind::Subscribed);
        assert_eq!(subscribed.url.as_deref(), Some("https://example.org/holidays.ics"));
        assert!(subscribed.read_only);
    }

    #[test]
    fn a_colour_chosen_here_survives_a_resync_of_the_list() {
        let conn = store();
        upsert_calendars(&conn, "acct", &[a_calendar()]).unwrap();
        set_calendar_color(&conn, "acct", "cal", Some("#E8830C")).unwrap();

        // The server does not know about colours and reports none, so keeping
        // the user's choice is the whole point of storing it separately.
        upsert_calendars(&conn, "acct", &[a_calendar()]).unwrap();

        assert_eq!(
            get_calendars(&conn, "acct").unwrap()[0].color.as_deref(),
            Some("#E8830C")
        );
    }

    #[test]
    fn a_window_sync_replaces_what_it_covers_and_leaves_the_rest() {
        let conn = store();
        upsert_calendars(&conn, "acct", &[a_calendar()]).unwrap();
        let day = 24 * 60 * 60;

        // Two windows, synced separately.
        replace_window(&conn, "acct", "cal", (0, 10 * day), &[event("a", day, day + 3600)])
            .unwrap();
        replace_window(
            &conn,
            "acct",
            "cal",
            (10 * day, 20 * day),
            &[event("b", 12 * day, 12 * day + 3600)],
        )
        .unwrap();
        assert_eq!(events_in_window(&conn, "acct", (0, 30 * day)).unwrap().len(), 2);

        // Re-syncing the first window with the occurrence gone — cancelled, or
        // moved out of range — must drop it, without touching the second.
        replace_window(&conn, "acct", "cal", (0, 10 * day), &[]).unwrap();
        let left = events_in_window(&conn, "acct", (0, 30 * day)).unwrap();
        assert_eq!(left.len(), 1);
        assert_eq!(left[0].id, "b", "the untouched window keeps its events");
    }

    #[test]
    fn a_window_covers_events_that_merely_overlap_it() {
        let conn = store();
        upsert_calendars(&conn, "acct", &[a_calendar()]).unwrap();
        let hour = 3600;

        // A meeting running from 23:00 to 01:00 belongs to both days it
        // touches; a query for either must find it.
        let overnight = event("night", 23 * hour, 25 * hour);
        replace_window(&conn, "acct", "cal", (0, 48 * hour), &[overnight]).unwrap();

        assert_eq!(
            events_in_window(&conn, "acct", (0, 24 * hour)).unwrap().len(),
            1,
            "found from the day it starts"
        );
        assert_eq!(
            events_in_window(&conn, "acct", (24 * hour, 48 * hour)).unwrap().len(),
            1,
            "and from the day it ends"
        );
        // A window that ends exactly when it starts holds nothing: the
        // boundaries are half-open, or a day view would show the previous
        // day's last meeting.
        assert!(events_in_window(&conn, "acct", (0, 23 * hour)).unwrap().is_empty());
    }

    #[test]
    fn events_of_a_hidden_calendar_are_not_returned() {
        let conn = store();
        upsert_calendars(&conn, "acct", &[a_calendar()]).unwrap();
        replace_window(&conn, "acct", "cal", (0, 3600), &[event("a", 0, 3600)]).unwrap();
        assert_eq!(events_in_window(&conn, "acct", (0, 3600)).unwrap().len(), 1);

        set_calendar_enabled(&conn, "acct", "cal", false).unwrap();
        assert!(
            events_in_window(&conn, "acct", (0, 3600)).unwrap().is_empty(),
            "hiding a calendar hides its events, without deleting them"
        );

        set_calendar_enabled(&conn, "acct", "cal", true).unwrap();
        assert_eq!(
            events_in_window(&conn, "acct", (0, 3600)).unwrap().len(),
            1,
            "and showing it again brings them straight back"
        );
    }

    #[test]
    fn participants_round_trip_through_the_cache() {
        let conn = store();
        upsert_calendars(&conn, "acct", &[a_calendar()]).unwrap();
        let mut meeting = event("m", 0, 3600);
        meeting.organizer = Some(Participant {
            name: "Pere".to_string(),
            addr: "pere@example.org".to_string(),
            response: "organizer".to_string(),
        });
        meeting.attendees = vec![Participant {
            name: String::new(),
            addr: "adil@example.org".to_string(),
            response: "accept".to_string(),
        }];
        meeting.is_recurring = true;
        meeting.free_busy = "busy".to_string();
        replace_window(&conn, "acct", "cal", (0, 3600), &[meeting.clone()]).unwrap();

        let stored = events_in_window(&conn, "acct", (0, 3600)).unwrap();
        assert_eq!(stored, vec![meeting]);
    }
}

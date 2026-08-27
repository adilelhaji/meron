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
               free_busy, my_response, organizer, attendees, series_id)
             VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16)",
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
                e.free_busy, e.my_response, e.organizer, e.attendees, e.series_id
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
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(events)
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

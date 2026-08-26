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

use anyhow::Result;
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};

/// A calendar an account exposes.
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
        // `enabled` is deliberately not overwritten: it is the user's choice,
        // and a resync of the calendar list must not silently re-show a
        // calendar they hid.
        tx.execute(
            "INSERT INTO calendars(account, provider_id, name, is_default, color)
             VALUES(?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(account, provider_id) DO UPDATE SET
               name = excluded.name,
               is_default = excluded.is_default,
               color = excluded.color",
            params![
                account,
                calendar.id,
                calendar.name,
                calendar.is_default as i64,
                calendar.color
            ],
        )?;
    }
    tx.commit()?;
    Ok(())
}

pub fn get_calendars(conn: &Connection, account: &str) -> Result<Vec<Calendar>> {
    let mut stmt = conn.prepare(
        "SELECT provider_id, name, is_default, enabled, color FROM calendars
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
               free_busy, my_response, organizer, attendees)
             VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)",
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
                e.free_busy, e.my_response, e.organizer, e.attendees
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
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(events)
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
            color: None,
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

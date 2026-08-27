//! Subscribed calendars: a published iCalendar file, fetched over HTTPS.
//!
//! Everywhere else a server expands recurring series for us, which is what
//! keeps recurrence rules out of this codebase. A published file has no server
//! behind it — it carries the rules themselves — so they have to be expanded
//! here. That work is delegated to an RFC 5545 implementation rather than
//! hand-rolled: recurrence with its exception dates, and the daylight-saving
//! transitions a series crosses, is exactly the arithmetic that looks simple
//! and is not.
//!
//! A subscription is read-only. The file belongs to whoever publishes it, and
//! there is nothing to write back to.

use anyhow::{Context as _, Result};
// `rrule` is reached through icalendar's reexport rather than as a direct
// dependency, so the two can never drift apart in version.
use icalendar::Tz as RRuleTz;
use icalendar::{Component, EventLike};

use super::{Event, Participant};

/// Cap on one fetch. A calendar file is small; a URL that streams forever is
/// not a calendar.
const FETCH_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(60);

/// Cap on the occurrences one series may contribute to a window, so a rule
/// with no end cannot fill the store.
const MAX_OCCURRENCES: u16 = 512;

/// Downloads a calendar file.
///
/// Blocking, like the rest of the crate's HTTP; callers wrap it in
/// `spawn_blocking`. Goes through the shared proxy-aware agent, so a
/// subscription honours the app's proxy like everything else.
pub fn fetch(url: &str) -> Result<String> {
    let mut response = crate::proxy::agent()?
        .get(url)
        .config()
        .http_status_as_error(false)
        .timeout_global(Some(FETCH_TIMEOUT))
        .build()
        .call()
        .with_context(|| format!("fetch calendar {url}"))?;
    let status = response.status().as_u16();
    if status != 200 {
        anyhow::bail!("calendar {url} answered HTTP {status}");
    }
    response
        .body_mut()
        .read_to_string()
        .with_context(|| format!("read calendar {url}"))
}

/// Parses a calendar file and returns the occurrences falling in a window,
/// with recurring series expanded.
///
/// A component that cannot be read is skipped rather than failing the file: a
/// published calendar is written by someone else's software, and one malformed
/// entry should not cost the user every other appointment in it.
pub fn parse_window(
    body: &str,
    calendar_id: &str,
    from: i64,
    to: i64,
) -> Result<(Vec<Event>, usize)> {
    let parsed: icalendar::Calendar = body
        .parse()
        .map_err(|err: String| anyhow::anyhow!("parse calendar: {err}"))?;

    let window_start = chrono::DateTime::from_timestamp(from, 0).context("window start")?;
    let window_end = chrono::DateTime::from_timestamp(to, 0).context("window end")?;

    let mut events = Vec::new();
    let mut skipped = 0usize;
    for component in parsed.components.iter() {
        let icalendar::CalendarComponent::Event(source) = component else {
            continue;
        };
        match expand(source, calendar_id, window_start, window_end) {
            Ok(mut occurrences) => events.append(&mut occurrences),
            Err(_) => skipped += 1,
        }
    }
    Ok((events, skipped))
}

/// Expands one event into the occurrences that fall inside the window.
fn expand(
    source: &icalendar::Event,
    calendar_id: &str,
    window_start: chrono::DateTime<chrono::Utc>,
    window_end: chrono::DateTime<chrono::Utc>,
) -> Result<Vec<Event>> {
    let uid = source.get_uid().unwrap_or_default().to_string();
    let subject = source.get_summary().unwrap_or_default().to_string();
    let location = source.get_location().map(str::to_string);
    let organizer = source.multi_properties().get("ORGANIZER").and_then(|list| {
        list.first().map(|property| Participant {
            name: property
                .params()
                .get("CN")
                .map(|param| param.value().to_string())
                .unwrap_or_default(),
            addr: property.value().trim_start_matches("mailto:").to_string(),
            response: String::new(),
        })
    });

    // The duration is taken from the first instance and applied to each
    // occurrence: RFC 5545 defines a series as one shape repeated, and this is
    // what keeps a two-hour meeting two hours long on every date.
    let (first_start, first_end) = span(source)?;
    let duration = (first_end - first_start).max(0);
    let all_day = matches!(source.get_start(), Some(icalendar::DatePerhapsTime::Date(_)));

    let recurrence = source
        .get_recurrence()
        .map_err(|err| anyhow::anyhow!("recurrence: {err}"))?;
    let repeats = uid_repeats(source);
    let expanded = recurrence
        .after(window_start.with_timezone(&RRuleTz::UTC))
        .before(window_end.with_timezone(&RRuleTz::UTC))
        .all(MAX_OCCURRENCES);

    Ok(expanded
        .dates
        .into_iter()
        .map(|occurrence| {
            let start = occurrence.timestamp();
            Event {
                // An occurrence is identified by its series and its instant:
                // the file gives one UID for the whole series, so a per-date
                // suffix is what keeps two dates from being one row.
                id: format!("{uid}@{start}"),
                calendar_id: calendar_id.to_string(),
                change_key: None,
                subject: subject.clone(),
                location: location.clone(),
                start,
                end: start + duration,
                all_day,
                is_recurring: repeats,
                // Every occurrence expanded above came from this one
                // component, so they all name it.
                series_id: repeats.then(|| uid.clone()),
                is_cancelled: source
                    .properties()
                    .get("STATUS")
                    .is_some_and(|status| status.value().eq_ignore_ascii_case("CANCELLED")),
                // A subscription is read-only and its publisher decides what
                // these mean; reporting a response we cannot give would be
                // inventing one.
                free_busy: String::new(),
                my_response: String::new(),
                organizer: organizer.clone(),
                attendees: Vec::new(),
            }
        })
        .collect())
}

/// Whether the component carries a recurrence rule of its own, as opposed to
/// being a single dated event.
fn uid_repeats(source: &icalendar::Event) -> bool {
    source.properties().contains_key("RRULE") || source.properties().contains_key("RDATE")
}

/// The first instance's start and end, as epoch seconds.
fn span(source: &icalendar::Event) -> Result<(i64, i64)> {
    let start = instant(source.get_start().context("event without a start")?);
    let end = source.get_end().map(instant).unwrap_or(start);
    Ok((start, end))
}

fn instant(value: icalendar::DatePerhapsTime) -> i64 {
    match value {
        icalendar::DatePerhapsTime::DateTime(datetime) => datetime
            .try_into_utc()
            .map(|utc| utc.timestamp())
            .unwrap_or_default(),
        // An all-day event starts at local midnight; the file gives no zone,
        // so UTC midnight is the only reading available here.
        icalendar::DatePerhapsTime::Date(date) => date
            .and_hms_opt(0, 0, 0)
            .map(|naive| naive.and_utc().timestamp())
            .unwrap_or_default(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A published calendar with a weekly series that crosses the end of
    /// daylight saving, one single event, and one entry too broken to read.
    const PUBLISHED: &str = "BEGIN:VCALENDAR\r\n\
VERSION:2.0\r\n\
PRODID:-//test//test//EN\r\n\
BEGIN:VEVENT\r\n\
UID:weekly@example.org\r\n\
DTSTART:20261001T080000Z\r\n\
DTEND:20261001T093000Z\r\n\
RRULE:FREQ=WEEKLY;COUNT=6\r\n\
SUMMARY:Standup\r\n\
LOCATION:Room 1\r\n\
END:VEVENT\r\n\
BEGIN:VEVENT\r\n\
UID:single@example.org\r\n\
DTSTART:20261005T120000Z\r\n\
DTEND:20261005T130000Z\r\n\
SUMMARY:Lunch\r\n\
END:VEVENT\r\n\
BEGIN:VEVENT\r\n\
UID:broken@example.org\r\n\
SUMMARY:No start at all\r\n\
END:VEVENT\r\n\
END:VCALENDAR\r\n";

    fn at(text: &str) -> i64 {
        chrono::DateTime::parse_from_rfc3339(text).unwrap().timestamp()
    }

    #[test]
    fn a_published_series_is_expanded_into_its_occurrences() {
        let (events, skipped) = parse_window(
            PUBLISHED,
            "cal",
            at("2026-10-01T00:00:00Z"),
            at("2026-11-30T00:00:00Z"),
        )
        .expect("the file should parse");

        // The entry with no start cannot be placed on a calendar, and is
        // skipped rather than costing the reader the other two.
        assert_eq!(skipped, 1, "the unreadable entry is skipped, not fatal");

        let standups: Vec<_> = events.iter().filter(|e| e.subject == "Standup").collect();
        assert_eq!(standups.len(), 6, "the weekly series is expanded");
        assert!(standups.iter().all(|e| e.is_recurring));
        assert!(standups.iter().all(|e| e.location.as_deref() == Some("Room 1")));

        // Each occurrence keeps the series' shape: 90 minutes, every time.
        assert!(standups.iter().all(|e| e.end - e.start == 90 * 60));

        // Occurrences of one series are distinct rows, not one row overwritten.
        let ids: std::collections::HashSet<_> = standups.iter().map(|e| &e.id).collect();
        assert_eq!(ids.len(), 6);

        let single: Vec<_> = events.iter().filter(|e| e.subject == "Lunch").collect();
        assert_eq!(single.len(), 1);
        assert!(!single[0].is_recurring, "a dated event is not a series");
    }

    #[test]
    fn a_window_only_yields_what_falls_inside_it() {
        // The series runs weekly from 1 October; a window over the first
        // fortnight must not return the later ones.
        let (events, _) = parse_window(
            PUBLISHED,
            "cal",
            at("2026-10-01T00:00:00Z"),
            at("2026-10-15T00:00:00Z"),
        )
        .expect("parse");
        let standups = events.iter().filter(|e| e.subject == "Standup").count();
        // 1 and 8 October only: the window ends at midnight on the 15th and
        // that day's occurrence is at 08:00, which is outside it.
        assert_eq!(standups, 2, "the window bounds the series, not the file");
    }

    #[test]
    fn a_file_that_is_not_a_calendar_fails_rather_than_reporting_nothing() {
        // Reporting an empty calendar for a 404 page would look like a working
        // subscription with no events in it.
        assert!(parse_window("<html>not a calendar</html>", "cal", 0, 1).is_err());
    }
}

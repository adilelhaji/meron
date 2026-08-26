//! Probes what an Exchange server will tell us about calendar events.
//!
//! Two questions decide how the calendar backend is built, and neither can be
//! answered from documentation:
//!
//!   1. Does `CalendarView` expand recurring events server-side, so we never
//!      have to implement recurrence rules ourselves?
//!   2. Can the event fields be read at all? The `ews` crate models a calendar
//!      item with the same struct as a mail message, which carries no start,
//!      end or location — so each candidate property is tried both as a typed
//!      field URI and as its MAPI extended property.
//!
//! Run against a real server (the smoke test's --probe handling passes the
//! password without putting it through the shell):
//!
//!   EWS_URL=... EWS_USER=... EWS_PASSWORD=... cargo run --release --example ews_calendar_probe

use ews::find_item::{FindItem, Traversal};
use ews::{
    BaseFolderId, BaseShape, DistinguishedPropertySet, ItemShape, PathToElement, PropertyType,
    RealItem, View,
};
use meron_core::exchange::{EwsClient, EwsConfig};

/// Typed field URIs the EWS schema is supposed to expose for appointments.
const TYPED: &[&str] = &[
    "item:Subject",
    "calendar:Start",
    "calendar:End",
    "calendar:Location",
    "calendar:IsAllDayEvent",
    "calendar:Organizer",
    "calendar:RequiredAttendees",
    "calendar:LegacyFreeBusyStatus",
    "calendar:IsRecurring",
    "calendar:MyResponseType",
];

/// The same values as MAPI properties in the Appointment set, which is the
/// fallback if the typed URIs are rejected or unreadable.
fn mapi_candidates() -> Vec<(&'static str, PathToElement)> {
    let appointment = |id: &str, kind: PropertyType| PathToElement::ExtendedFieldURI {
        distinguished_property_set_id: Some(DistinguishedPropertySet::Appointment),
        property_set_id: None,
        property_tag: None,
        property_name: None,
        property_id: Some(id.to_string()),
        property_type: kind,
    };
    vec![
        ("PidLidAppointmentStartWhole 0x820D", appointment("0x820D", PropertyType::SystemTime)),
        ("PidLidAppointmentEndWhole   0x820E", appointment("0x820E", PropertyType::SystemTime)),
        ("PidLidLocation              0x8208", appointment("0x8208", PropertyType::String)),
        ("PidLidAppointmentSubType    0x8215", appointment("0x8215", PropertyType::Boolean)),
    ]
}

fn env(name: &str) -> String {
    std::env::var(name).unwrap_or_else(|_| panic!("{name} must be set"))
}

/// A FindItem over the calendar for `days` ahead, asking for `properties`.
fn calendar_view(
    client: &EwsClient,
    days: i64,
    properties: Vec<PathToElement>,
) -> anyhow::Result<Vec<::ews::Message>> {
    // EWS wants UTC in this format; a fixed window keeps the probe deterministic
    // enough to compare runs.
    let now = chrono::Utc::now();
    let start = now.format("%Y-%m-%dT%H:%M:%SZ").to_string();
    let end = (now + chrono::Duration::days(days))
        .format("%Y-%m-%dT%H:%M:%SZ")
        .to_string();
    let response = client.call(FindItem {
        traversal: Traversal::Shallow,
        item_shape: ItemShape {
            base_shape: BaseShape::IdOnly,
            include_mime_content: None,
            additional_properties: if properties.is_empty() {
                None
            } else {
                Some(properties)
            },
        },
        view: Some(View::CalendarView {
            max_entries_returned: Some(50),
            start_date: start,
            end_date: end,
        }),
        parent_folder_ids: vec![BaseFolderId::DistinguishedFolderId {
            id: "calendar".to_string(),
            change_key: None,
            mailbox: None,
        }],
    })?;
    let mut events = Vec::new();
    for message in ::ews::OperationResponse::into_response_messages(response) {
        let (::ews::ResponseClass::Success(message) | ::ews::ResponseClass::Warning(message)) =
            message
        else {
            continue;
        };
        for item in message.root_folder.items.inner {
            if let RealItem::CalendarItem(event) | RealItem::Message(event) = item {
                events.push(event);
            }
        }
    }
    Ok(events)
}

fn main() -> anyhow::Result<()> {
    let client = EwsClient::new(EwsConfig {
        url: env("EWS_URL"),
        username: env("EWS_USER"),
        password: env("EWS_PASSWORD"),
    });

    // A wide window is what settles whether the server expands a series: a
    // weekly meeting should return one occurrence per week, not one master.
    let days: i64 = std::env::var("EWS_DAYS")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(14);

    println!("\n1. does the calendar answer at all, and does it expand series?");
    let events = calendar_view(&client, days, Vec::new())?;
    println!("   {} occurrence(s) in the next {days} days", events.len());
    if events.is_empty() {
        println!("   (an empty calendar cannot answer question 2 — try a wider window)");
    }

    println!("\n2. which properties does the server accept, one at a time?");
    let mut accepted_typed = Vec::new();
    for name in TYPED {
        let property = PathToElement::FieldURI {
            field_URI: name.to_string(),
        };
        match calendar_view(&client, days, vec![property]) {
            Ok(_) => {
                println!("   ok       {name}");
                accepted_typed.push(*name);
            }
            Err(err) => println!("   REJECTED {name}  ({err})"),
        }
    }
    for (label, property) in mapi_candidates() {
        match calendar_view(&client, days, vec![property]) {
            Ok(_) => println!("   ok       MAPI {label}"),
            Err(err) => println!("   REJECTED MAPI {label}  ({err})"),
        }
    }

    println!("\n3. what actually comes back, read through the crate's types?");
    let properties: Vec<PathToElement> = accepted_typed
        .iter()
        .map(|name| PathToElement::FieldURI {
            field_URI: name.to_string(),
        })
        .collect();
    let events = calendar_view(&client, days, properties)?;
    for event in events.iter().take(10) {
        println!(
            "   {} .. {}  {:?}{}",
            event.start.as_ref().map_or("?".into(), |d| d.0.to_string()),
            event.end.as_ref().map_or("?".into(), |d| d.0.to_string()),
            event.subject.as_deref().unwrap_or("(sin asunto)"),
            match (&event.location, event.is_recurring) {
                (Some(place), Some(true)) => format!("  @{place} [serie]"),
                (Some(place), _) => format!("  @{place}"),
                (None, Some(true)) => "  [serie]".to_string(),
                _ => String::new(),
            },
        );
        if let Some(organizer) = &event.organizer {
            println!(
                "       organiza: {}",
                organizer.mailbox.email_address.as_deref().unwrap_or("?")
            );
        }
    }
    println!(
        "\n   {} event(s) read; recurrence expansion is the server's job if the\n   count above exceeds the number of distinct series in your calendar.\n",
        events.len()
    );
    Ok(())
}

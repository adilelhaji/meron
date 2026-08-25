//! Probes which EWS envelope properties an Exchange server accepts.
//!
//! Exchange answers a rejected property with a bare `ErrorInvalidRequest`
//! naming nothing, so the only way to find the offender is to ask for one
//! property at a time. Run against a real server:
//!
//!   EWS_URL=https://mail.example.org/EWS/Exchange.asmx \
//!   EWS_USER='DOMAIN\user' EWS_PASSWORD=... EWS_FOLDER='Inbox' \
//!   cargo run --release --example ews_probe

use ews::get_item::GetItem;
use ews::{BaseItemId, BaseShape, ItemShape, PathToElement};
use meron_core::exchange::{EwsClient, EwsConfig};

/// Every property the envelope fetch would like to request.
const CANDIDATES: &[&str] = &[
    "item:Subject",
    "item:DateTimeSent",
    "item:DateTimeReceived",
    "message:From",
    "message:Sender",
    "message:ToRecipients",
    "message:CcRecipients",
    "message:IsRead",
    "message:InternetMessageId",
    "message:InReplyTo",
    "message:References",
    "message:ConversationIndex",
    "message:ConversationTopic",
    "item:InternetMessageHeaders",
];

fn env(name: &str) -> String {
    std::env::var(name).unwrap_or_else(|_| panic!("{name} must be set"))
}

fn main() -> anyhow::Result<()> {
    let client = EwsClient::new(EwsConfig {
        url: env("EWS_URL"),
        username: env("EWS_USER"),
        password: env("EWS_PASSWORD"),
    });
    let wanted_folder = std::env::var("EWS_FOLDER").unwrap_or_else(|_| "Inbox".to_string());

    // Find the folder, then one item in it to probe against.
    let hierarchy = client.folder_hierarchy(None)?;
    let mut folder = None;
    for change in hierarchy.changes.inner {
        if let ews::sync_folder_hierarchy::Change::Create { folder: created } = change
            && let ews::Folder::Folder {
                folder_id: Some(id),
                display_name: Some(name),
                ..
            } = created
            && name == wanted_folder
        {
            folder = Some((id.id.clone(), id.change_key.clone()));
        }
    }
    let folder = folder.unwrap_or_else(|| panic!("no folder named {wanted_folder}"));

    let round = client.item_sync(&folder, None, 8)?;
    let item = round
        .changes
        .inner
        .into_iter()
        .find_map(|change| match change {
            ews::sync_folder_items::Change::Create { item } => {
                item.inner_message().item_id.clone()
            }
            _ => None,
        })
        .expect("folder has no items to probe");
    println!("probing against one item in {wanted_folder}\n");

    let mut accepted = Vec::new();
    for property in CANDIDATES {
        let result = client.call(GetItem {
            item_shape: ItemShape {
                base_shape: BaseShape::IdOnly,
                include_mime_content: None,
                additional_properties: Some(vec![PathToElement::FieldURI {
                    field_URI: property.to_string(),
                }]),
            },
            item_ids: vec![BaseItemId::ItemId {
                id: item.id.clone(),
                change_key: None,
            }],
        });
        match result {
            Ok(_) => {
                println!("  ok       {property}");
                accepted.push(*property);
            }
            Err(err) => println!("  REJECTED {property}  ({err})"),
        }
    }

    println!("\naccepted together:");
    let combined = client.call(GetItem {
        item_shape: ItemShape {
            base_shape: BaseShape::IdOnly,
            include_mime_content: None,
            additional_properties: Some(
                accepted
                    .iter()
                    .map(|p| PathToElement::FieldURI {
                        field_URI: p.to_string(),
                    })
                    .collect(),
            ),
        },
        item_ids: vec![BaseItemId::ItemId {
            id: item.id,
            change_key: None,
        }],
    });
    match combined {
        Ok(_) => println!("  ok  all {} accepted properties in one request", accepted.len()),
        Err(err) => println!("  FAILED as a set: {err}"),
    }
    Ok(())
}

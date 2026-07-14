use serde::Serialize;
use vivarium::VivariumError;
use vivarium::mailspace::{MailAbsorbFilter, Mailspace};
use vivarium::storage::{MailspaceEvent, StoredMessageView};

#[derive(Debug, Serialize)]
struct MailListItem {
    handle: String,
    date: String,
    from: String,
    to: String,
    subject: String,
    role: String,
    absorbed: bool,
    absorbed_by: Option<String>,
}

pub(crate) fn print_mail_list(
    mailspace: &Mailspace,
    identity: &str,
    role: &str,
    absorb_status: MailAbsorbFilter,
    absorbed_by: &Option<String>,
    json: bool,
) -> Result<(), VivariumError> {
    let storage = mailspace.storage()?;
    let mut items = Vec::new();
    for message in mailspace.list(identity, role)? {
        let events = storage.list_mailspace_events(&message.message_id)?;
        if matches_absorb(&events, absorb_status, absorbed_by) {
            items.push(mail_list_item(message, &events));
        }
    }
    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&items)
                .map_err(|e| VivariumError::Other(format!("failed to encode JSON: {e}")))?
        );
        return Ok(());
    }
    if items.is_empty() {
        println!("  no messages in {role}");
        return Ok(());
    }
    for item in &items {
        println!(
            "  {}  {}  {}  {}  absorbed={}",
            item.handle, item.date, item.from, item.subject, item.absorbed
        );
    }
    Ok(())
}

fn mail_list_item(message: StoredMessageView, events: &[MailspaceEvent]) -> MailListItem {
    let absorbed_by = events
        .iter()
        .rev()
        .find(|event| event.command == "mail absorb")
        .and_then(|event| event.actor_identity.clone());
    MailListItem {
        handle: message.handle,
        date: message.date,
        from: message.from_addr,
        to: message.to_addr,
        subject: message.subject,
        role: message.local_role,
        absorbed: absorbed_by.is_some(),
        absorbed_by,
    }
}

fn matches_absorb(
    events: &[MailspaceEvent],
    absorb_status: MailAbsorbFilter,
    absorbed_by: &Option<String>,
) -> bool {
    let absorbed = events.iter().any(|event| event.command == "mail absorb");
    let status_matches = match absorb_status {
        MailAbsorbFilter::All => true,
        MailAbsorbFilter::Absorbed => absorbed,
        MailAbsorbFilter::Unabsorbed => !absorbed,
    };
    status_matches
        && absorbed_by.as_ref().is_none_or(|identity| {
            events.iter().any(|event| {
                event.command == "mail absorb" && event.actor_identity.as_ref() == Some(identity)
            })
        })
}

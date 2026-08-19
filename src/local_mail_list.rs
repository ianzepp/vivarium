use serde::Serialize;
use vivarium::VivariumError;
use vivarium::cli::MailListCommand;
use vivarium::mailspace::{MailAbsorbFilter, Mailspace, canonical_local_role};
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
    command: &MailListCommand,
    absorb_status: MailAbsorbFilter,
) -> Result<(), VivariumError> {
    let storage = mailspace.storage()?;
    let messages = match command.for_identity.as_deref() {
        Some(identity) => mailspace.list(identity, &command.folder)?,
        None => storage.list_messages_by_role(&canonical_local_role(&command.folder)?)?,
    };
    let from = resolve_list_header(mailspace, command.from.as_deref());
    let to = resolve_list_header(mailspace, command.to.as_deref());
    let mut items = Vec::new();
    for message in messages {
        if !headers_match(
            &message.from_addr,
            &message.to_addr,
            &message.cc_addr,
            from.as_deref(),
            to.as_deref(),
        ) {
            continue;
        }
        let events = storage.list_mailspace_events(&message.message_id)?;
        if matches_absorb(
            &message,
            &events,
            absorb_status,
            command.absorbed_by.as_ref(),
        ) {
            items.push(mail_list_item(message, &events));
        }
    }
    if command.json {
        println!(
            "{}",
            serde_json::to_string_pretty(&items)
                .map_err(|e| VivariumError::Other(format!("failed to encode JSON: {e}")))?
        );
        return Ok(());
    }
    if items.is_empty() {
        let folder = &command.folder;
        println!("  no messages in {folder}");
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

pub(crate) fn resolve_list_header(mailspace: &Mailspace, raw: Option<&str>) -> Option<String> {
    let raw = raw.map(str::trim).filter(|value| !value.is_empty())?;
    Some(
        mailspace
            .resolve_identity(raw)
            .map_or_else(|_| raw.to_string(), |name| mailspace.address_for(&name))
            .to_ascii_lowercase(),
    )
}

pub(crate) fn headers_match(
    from_addr: &str,
    to_addr: &str,
    cc_addr: &str,
    from: Option<&str>,
    to: Option<&str>,
) -> bool {
    let hay_from = from_addr.to_ascii_lowercase();
    let hay_to = to_addr.to_ascii_lowercase();
    let hay_cc = cc_addr.to_ascii_lowercase();
    from.is_none_or(|needle| hay_from.contains(needle))
        && to.is_none_or(|needle| hay_to.contains(needle) || hay_cc.contains(needle))
}

fn mail_list_item(message: StoredMessageView, events: &[MailspaceEvent]) -> MailListItem {
    let absorbed_by = message.absorbed_by.clone().or_else(|| {
        events
            .iter()
            .rev()
            .find(|event| event.event_type == "absorbed" || event.command.ends_with(" absorb"))
            .and_then(|event| event.actor_identity.clone())
    });
    MailListItem {
        handle: message.handle,
        date: message.date,
        from: message.from_addr,
        to: message.to_addr,
        subject: message.subject,
        role: message.local_role,
        absorbed: message.absorbed_at.is_some() || absorbed_by.is_some(),
        absorbed_by,
    }
}

fn matches_absorb(
    message: &StoredMessageView,
    events: &[MailspaceEvent],
    absorb_status: MailAbsorbFilter,
    absorbed_by: Option<&String>,
) -> bool {
    let absorbed = message.absorbed_at.is_some()
        || events
            .iter()
            .any(|event| event.event_type == "absorbed" || event.command.ends_with(" absorb"));
    let status_matches = match absorb_status {
        MailAbsorbFilter::All => true,
        MailAbsorbFilter::Absorbed => absorbed,
        MailAbsorbFilter::Unabsorbed => !absorbed,
    };
    status_matches
        && absorbed_by.is_none_or(|identity| {
            message.absorbed_by.as_ref() == Some(identity)
                || events.iter().any(|event| {
                    (event.event_type == "absorbed" || event.command.ends_with(" absorb"))
                        && event.actor_identity.as_ref() == Some(identity)
                })
        })
}

use serde::Serialize;
use vivarium::VivariumError;
use vivarium::mailspace::Mailspace;
use vivarium::storage::{MailspaceEvent, StoredMessageView};

#[derive(Debug, Serialize)]
struct WorkListItem {
    handle: String,
    kind: String,
    status: String,
    role: String,
    date: String,
    from: String,
    to: String,
    subject: String,
    last_event: Option<WorkListEvent>,
}

#[derive(Debug, Serialize)]
struct WorkListEvent {
    occurred_at: String,
    command: String,
    event_type: String,
    actor_identity: Option<String>,
    note: Option<String>,
}

pub(crate) fn print_work_list(
    mailspace: &Mailspace,
    identity: &str,
    role: &str,
    kind: &str,
    json: bool,
) -> Result<(), VivariumError> {
    print_work_lists(mailspace, identity, &[role], kind, json)
}

pub(crate) fn print_work_lists(
    mailspace: &Mailspace,
    identity: &str,
    roles: &[&str],
    kind: &str,
    json: bool,
) -> Result<(), VivariumError> {
    let storage = mailspace.storage()?;
    let mut messages = Vec::new();
    for role in roles {
        messages.extend(mailspace.list_kind(identity, role, kind)?);
    }
    let message_ids = messages
        .iter()
        .map(|message| message.message_id.clone())
        .collect::<Vec<_>>();
    let events_by_message = storage.list_mailspace_events_for_messages(&message_ids)?;
    let items = messages
        .into_iter()
        .map(|message| {
            let events = events_by_message
                .get(&message.message_id)
                .map_or([].as_slice(), Vec::as_slice);
            work_list_item(message, kind, events)
        })
        .collect::<Vec<_>>();
    if json {
        print_json(&items)
    } else {
        print_human(kind, roles, &items);
        Ok(())
    }
}

fn work_list_item(
    message: StoredMessageView,
    kind: &str,
    events: &[MailspaceEvent],
) -> WorkListItem {
    WorkListItem {
        handle: message.handle,
        kind: kind.into(),
        status: status_for_role(&message.local_role),
        role: message.local_role,
        date: message.date,
        from: message.from_addr,
        to: message.to_addr,
        subject: message.subject,
        last_event: events.last().map(work_list_event),
    }
}

fn work_list_event(event: &MailspaceEvent) -> WorkListEvent {
    WorkListEvent {
        occurred_at: event.occurred_at.clone(),
        command: event.command.clone(),
        event_type: event.event_type.clone(),
        actor_identity: event.actor_identity.clone(),
        note: event.note.clone(),
    }
}

fn status_for_role(role: &str) -> String {
    if role == "done" {
        "done".into()
    } else {
        "open".into()
    }
}

fn print_json(items: &[WorkListItem]) -> Result<(), VivariumError> {
    println!(
        "{}",
        serde_json::to_string_pretty(items)
            .map_err(|e| VivariumError::Other(format!("failed to encode JSON: {e}")))?
    );
    Ok(())
}

fn print_human(kind: &str, roles: &[&str], items: &[WorkListItem]) {
    if items.is_empty() {
        let role = roles.join("/");
        println!("  no {kind}s in {role}");
        return;
    }
    println!("  handle  status  role  date  from  subject  last_event");
    for item in items {
        let last_event = item
            .last_event
            .as_ref()
            .map(|event| event.command.as_str())
            .unwrap_or("-");
        println!(
            "  {}  {}  {}  {}  {}  {}  {}",
            item.handle, item.status, item.role, item.date, item.from, item.subject, last_event
        );
    }
}

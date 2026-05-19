use super::SendRequest;
use super::delivery::DeliveredMessageId;
use crate::error::VivariumError;
use crate::storage::{MailspaceEventInput, Storage, StoredMessage, StoredMessageView};

pub(super) fn log_raw_delivery_event(
    storage: &Storage,
    recipient: &str,
    role: &str,
    stored: &StoredMessage,
    parsed: &mail_parser::Message<'_>,
) -> Result<(), VivariumError> {
    append_event(
        storage,
        EventDetails {
            command: "mail deliver",
            event_type: "delivered",
            actor: None,
            account: recipient,
            message_id: &stored.message_id,
            content_id: &stored.content_id,
            from_role: None,
            to_role: Some(role),
            from_identity: None,
            to_identity: Some(recipient),
            subject: parsed.subject().unwrap_or_default(),
            note: None,
        },
    )
}

pub(super) fn log_send_events(
    storage: &Storage,
    from: &str,
    request: &SendRequest,
    delivered: &[DeliveredMessageId],
    sent: &StoredMessage,
) -> Result<(), VivariumError> {
    let command = send_command(&request.role);
    append_event(
        storage,
        EventDetails {
            command,
            event_type: "sent_copy_created",
            actor: Some(from),
            account: from,
            message_id: &sent.message_id,
            content_id: &sent.content_id,
            from_role: None,
            to_role: Some("sent"),
            from_identity: Some(from),
            to_identity: Some(from),
            subject: &request.subject,
            note: None,
        },
    )?;
    for delivered in delivered {
        append_event(
            storage,
            EventDetails {
                command,
                event_type: "delivered",
                actor: Some(from),
                account: &delivered.identity,
                message_id: &delivered.message_id,
                content_id: &delivered.content_id,
                from_role: None,
                to_role: Some(&request.role),
                from_identity: Some(from),
                to_identity: Some(&delivered.identity),
                subject: &request.subject,
                note: None,
            },
        )?;
    }
    Ok(())
}

pub(super) fn log_move_event(
    storage: &Storage,
    identity: &str,
    role: &str,
    before: &StoredMessageView,
    command: &str,
    note: Option<&str>,
) -> Result<(), VivariumError> {
    append_event(
        storage,
        EventDetails {
            command,
            event_type: "moved",
            actor: Some(identity),
            account: identity,
            message_id: &before.message_id,
            content_id: &before.content_id,
            from_role: Some(&before.local_role),
            to_role: Some(role),
            from_identity: Some(identity),
            to_identity: Some(identity),
            subject: &before.subject,
            note,
        },
    )
}

fn send_command(role: &str) -> &'static str {
    match role {
        "tasks" => "task send",
        "needs" => "need send",
        "wants" => "want send",
        _ => "mail send",
    }
}

struct EventDetails<'a> {
    command: &'a str,
    event_type: &'a str,
    actor: Option<&'a str>,
    account: &'a str,
    message_id: &'a str,
    content_id: &'a str,
    from_role: Option<&'a str>,
    to_role: Option<&'a str>,
    from_identity: Option<&'a str>,
    to_identity: Option<&'a str>,
    subject: &'a str,
    note: Option<&'a str>,
}

fn append_event(storage: &Storage, details: EventDetails<'_>) -> Result<(), VivariumError> {
    storage.append_mailspace_event(&MailspaceEventInput {
        command: details.command.into(),
        event_type: details.event_type.into(),
        actor_identity: details.actor.map(str::to_string),
        account: details.account.into(),
        message_id: details.message_id.into(),
        content_id: details.content_id.into(),
        from_role: details.from_role.map(str::to_string),
        to_role: details.to_role.map(str::to_string),
        from_identity: details.from_identity.map(str::to_string),
        to_identity: details.to_identity.map(str::to_string),
        subject: details.subject.into(),
        note: details.note.map(str::to_string),
    })?;
    Ok(())
}

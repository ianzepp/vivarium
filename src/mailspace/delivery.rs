use std::collections::BTreeSet;

use chrono::Utc;

use super::{DeliveredMessage, DeliveryResult, Mailspace, SendRequest, canonical_local_role};
use crate::error::VivariumError;
use crate::message::{ComposeDraft, build_compose_draft, validate_message_headers};
use crate::storage::{
    MailspaceEventInput, MessageIngestRequest, Storage, StoredMessage, StoredMessageView,
};

struct DeliveredMessageId {
    identity: String,
    message_id: String,
    content_id: String,
}

impl Mailspace {
    pub fn send(&self, request: SendRequest) -> Result<DeliveryResult, VivariumError> {
        if !request.bcc.is_empty() {
            return Err(VivariumError::Message(
                "Bcc is not supported for local mailspace delivery in v1".into(),
            ));
        }
        let from = self.resolve_identity(&request.from)?;
        let recipients = self.resolve_recipients(&request.to, &request.cc)?;
        if recipients.is_empty() {
            return Err(VivariumError::Message(
                "local delivery needs at least one To or Cc recipient".into(),
            ));
        }
        let eml = self.compose_message(&from, &request)?;
        let mut storage = self.storage()?;
        let seed = Utc::now().timestamp_nanos_opt().unwrap_or_default();
        let delivered_ids =
            ingest_for_recipients(&mut storage, recipients, &request.role, &eml, seed)?;
        let sent = ingest_sent_copy(&mut storage, &from, &eml, seed)?;
        log_send_events(&storage, &from, &request, &delivered_ids, &sent)?;
        let delivered = delivered_with_handles(&storage, delivered_ids)?;
        Ok(DeliveryResult {
            delivered,
            sent: storage.display_handle(&sent.message_id)?,
        })
    }

    fn compose_message(&self, from: &str, request: &SendRequest) -> Result<String, VivariumError> {
        let to = self.addresses_for(&request.to)?;
        let cc = self.addresses_for(&request.cc)?;
        let mut eml = build_compose_draft(&ComposeDraft {
            from: self.address_for(from),
            to,
            cc,
            bcc: Vec::new(),
            subject: request.subject.clone(),
            body: request.body.clone(),
            html_body: None,
        })?;
        if let Some(kind) = &request.kind {
            eml = add_header(&eml, "X-Vivi-Kind", kind)?;
        }
        Ok(eml)
    }

    fn addresses_for(&self, values: &[String]) -> Result<Vec<String>, VivariumError> {
        values
            .iter()
            .map(|value| self.resolve_identity(value).map(|id| self.address_for(&id)))
            .collect()
    }

    pub fn deliver_raw(
        &self,
        data: &[u8],
        folder: &str,
    ) -> Result<Vec<DeliveredMessage>, VivariumError> {
        validate_message_headers(data)?;
        let parsed = mail_parser::MessageParser::default()
            .parse(data)
            .ok_or_else(|| VivariumError::Parse("failed to parse message".into()))?;
        if parsed.bcc().is_some_and(|a| a.first().is_some()) {
            return Err(VivariumError::Message(
                "Bcc is not supported for local mailspace delivery in v1".into(),
            ));
        }
        let mut recipients = BTreeSet::new();
        collect_addresses(parsed.to(), &mut recipients);
        collect_addresses(parsed.cc(), &mut recipients);
        let recipients = recipients
            .iter()
            .map(|addr| self.resolve_identity(addr))
            .collect::<Result<Vec<_>, _>>()?;
        let role = canonical_local_role(folder)?;
        let mut storage = self.storage()?;
        let mut delivered = Vec::new();
        for recipient in recipients {
            let stored = storage.ingest_message(
                &MessageIngestRequest {
                    account: recipient.clone(),
                    local_role: role.clone(),
                    read_state: false,
                    starred: false,
                    message_id_hint: None,
                    seed_hint: format!("raw-delivery\0{recipient}\0{}", data.len()),
                    remote: None,
                },
                data,
            )?;
            log_raw_delivery_event(&storage, &recipient, &role, &stored, &parsed)?;
            delivered.push(DeliveredMessage {
                identity: recipient,
                handle: storage.display_handle(&stored.message_id)?,
            });
        }
        Ok(delivered)
    }

    pub fn list(
        &self,
        identity: &str,
        role: &str,
    ) -> Result<Vec<StoredMessageView>, VivariumError> {
        let identity = self.resolve_identity(identity)?;
        let role = canonical_local_role(role)?;
        let storage = self.storage()?;
        Ok(storage
            .list_messages_by_role(&role)?
            .into_iter()
            .filter(|message| message.account == identity)
            .collect())
    }

    pub fn move_task(
        &self,
        identity: &str,
        handle: &str,
        role: &str,
        note: Option<&str>,
    ) -> Result<String, VivariumError> {
        let identity = self.resolve_identity(identity)?;
        let role = canonical_local_role(role)?;
        let mut storage = self.storage()?;
        let resolved = storage.resolve_message_token(handle)?;
        let Some(before) = storage.message_by_id(&resolved)? else {
            return Err(VivariumError::Message(format!(
                "message not found: {handle}"
            )));
        };
        storage.move_message_to_role(&identity, &resolved, &role)?;
        append_event(
            &storage,
            EventDetails {
                command: if role == "done" {
                    "task done"
                } else {
                    "task reopen"
                },
                event_type: "moved",
                actor: Some(&identity),
                account: &identity,
                message_id: &before.message_id,
                content_id: &before.content_id,
                from_role: Some(&before.local_role),
                to_role: Some(&role),
                from_identity: Some(&identity),
                to_identity: Some(&identity),
                subject: &before.subject,
                note,
            },
        )?;
        storage.display_handle(&resolved)
    }

    fn resolve_recipients(
        &self,
        to: &[String],
        cc: &[String],
    ) -> Result<BTreeSet<String>, VivariumError> {
        let mut recipients = BTreeSet::new();
        for value in to.iter().chain(cc) {
            recipients.insert(self.resolve_identity(value)?);
        }
        Ok(recipients)
    }
}

fn add_header(eml: &str, name: &str, value: &str) -> Result<String, VivariumError> {
    let newline = if eml.contains("\r\n") { "\r\n" } else { "\n" };
    let separator = format!("{newline}{newline}");
    let (headers, body) = eml
        .split_once(&separator)
        .ok_or_else(|| VivariumError::Message("message has no header/body separator".into()))?;
    Ok(format!(
        "{headers}{newline}{name}: {value}{separator}{body}"
    ))
}

fn ingest_for_recipients(
    storage: &mut Storage,
    recipients: BTreeSet<String>,
    role: &str,
    eml: &str,
    seed: i64,
) -> Result<Vec<DeliveredMessageId>, VivariumError> {
    let mut delivered = Vec::new();
    for recipient in recipients {
        let stored = storage.ingest_message(
            &MessageIngestRequest {
                account: recipient.clone(),
                local_role: role.to_string(),
                read_state: false,
                starred: false,
                message_id_hint: None,
                seed_hint: format!("local-delivery\0{seed}\0{recipient}"),
                remote: None,
            },
            eml.as_bytes(),
        )?;
        delivered.push(DeliveredMessageId {
            identity: recipient,
            message_id: stored.message_id,
            content_id: stored.content_id,
        });
    }
    Ok(delivered)
}

fn delivered_with_handles(
    storage: &Storage,
    delivered: Vec<DeliveredMessageId>,
) -> Result<Vec<DeliveredMessage>, VivariumError> {
    delivered
        .into_iter()
        .map(|message| {
            Ok(DeliveredMessage {
                identity: message.identity,
                handle: storage.display_handle(&message.message_id)?,
            })
        })
        .collect()
}

fn ingest_sent_copy(
    storage: &mut Storage,
    from: &str,
    eml: &str,
    seed: i64,
) -> Result<StoredMessage, VivariumError> {
    storage.ingest_message(
        &MessageIngestRequest {
            account: from.to_string(),
            local_role: "sent".into(),
            read_state: true,
            starred: false,
            message_id_hint: None,
            seed_hint: format!("local-sent\0{seed}"),
            remote: None,
        },
        eml.as_bytes(),
    )
}

fn log_raw_delivery_event(
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

fn log_send_events(
    storage: &Storage,
    from: &str,
    request: &SendRequest,
    delivered: &[DeliveredMessageId],
    sent: &StoredMessage,
) -> Result<(), VivariumError> {
    let command = if request.role == "tasks" {
        "task send"
    } else {
        "mail send"
    };
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

fn collect_addresses<'a>(
    addresses: Option<&'a mail_parser::Address<'a>>,
    out: &mut BTreeSet<String>,
) {
    let Some(addresses) = addresses else {
        return;
    };
    for addr in addresses.iter() {
        if let Some(address) = addr.address.as_deref() {
            out.insert(address.to_string());
        }
    }
}

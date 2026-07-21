use std::collections::BTreeSet;

use chrono::Utc;

use super::event_log::{log_move_event, log_send_events};
use super::kind::matches_kind;
use super::reply::{add_reply_headers, resolve_reply_parent};
use super::{DeliveredMessage, DeliveryResult, Mailspace, SendRequest, canonical_local_role};
use crate::error::VivariumError;
use crate::message::{ComposeDraft, build_compose_draft, validate_message_headers};
use crate::storage::{
    MailspaceEventInput, MailspaceMoveWithReply, MessageIngestRequest, Storage, StoredMessage,
    StoredMessageView,
};

pub(super) struct DeliveredMessageId {
    pub(super) identity: String,
    pub(super) message_id: String,
    pub(super) content_id: String,
}

impl Mailspace {
    /// Send a message to identities in the mailspace. `request` specifies the
    /// sender, recipients, subject, body, and optional role. Sent and delivered
    /// copies are created in the store.
    ///
    /// # Errors
    /// Returns an error if the sender or any recipient identity cannot be
    /// resolved, if message composition fails, or if a storage operation fails.
    #[allow(clippy::needless_pass_by_value)]
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
        let mut storage = self.storage()?;
        let reply_parent = request
            .reply_to
            .as_deref()
            .map(|handle| resolve_reply_parent(&storage, handle))
            .transpose()?;
        let eml = self.compose_message(
            &from,
            &request,
            reply_parent.as_ref().map(|(_, data)| data.as_slice()),
        )?;
        let seed = Utc::now().timestamp_nanos_opt().unwrap_or_default();
        let delivered_ids =
            ingest_for_recipients(&mut storage, recipients, &request.role, &eml, seed)?;
        let sent = ingest_sent_copy(&mut storage, &from, &eml, seed)?;
        if let Some((parent, _)) = reply_parent {
            storage.link_mailspace_content(&sent.content_id, &parent.content_id, "captured")?;
        }
        log_send_events(&storage, &from, &request, &delivered_ids, &sent)?;
        let delivered = delivered_with_handles(&storage, delivered_ids)?;
        Ok(DeliveryResult {
            delivered,
            sent: storage.display_handle(&sent.message_id)?,
        })
    }

    /// Save a self-addressed memo into the identity's `memos` folder.
    /// Unlike `send`, this creates no sent copy and no recipient delivery —
    /// it is a single-actor persistence operation.
    ///
    /// # Errors
    /// Returns an error if the identity cannot be resolved, message composition
    /// fails, or the storage operation fails.
    pub fn save_memo(
        &self,
        identity: &str,
        subject: &str,
        body: &str,
    ) -> Result<String, VivariumError> {
        let identity = self.resolve_identity(identity)?;
        let address = self.address_for(&identity);
        let mut eml = build_compose_draft(&ComposeDraft {
            from: address.clone(),
            to: vec![address],
            cc: Vec::new(),
            bcc: Vec::new(),
            subject: subject.to_string(),
            body: body.to_string(),
            html_body: None,
        })?;
        eml = add_header(&eml, "X-Vivi-Kind", "memo")?;
        let seed = Utc::now().timestamp_nanos_opt().unwrap_or_default();
        let mut storage = self.storage()?;
        let stored = storage.ingest_message(
            &MessageIngestRequest {
                account: identity.clone(),
                local_role: "memos".into(),
                read_state: false,
                starred: false,
                message_id_hint: None,
                seed_hint: format!("memo-save\0{seed}\0{identity}"),
                remote: None,
            },
            eml.as_bytes(),
        )?;
        let handle = storage.display_handle(&stored.message_id)?;
        Ok(handle)
    }

    fn compose_message(
        &self,
        from: &str,
        request: &SendRequest,
        parent: Option<&[u8]>,
    ) -> Result<String, VivariumError> {
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
        if let Some(parent) = parent {
            eml = add_reply_headers(&eml, parent)?;
        }
        Ok(eml)
    }

    pub(super) fn addresses_for(&self, values: &[String]) -> Result<Vec<String>, VivariumError> {
        values
            .iter()
            .map(|value| self.resolve_identity(value).map(|id| self.address_for(&id)))
            .collect()
    }

    /// Deliver a raw `.eml` message into the mailspace for each recipient that
    /// resolves to a known local identity.
    ///
    /// # Errors
    /// Returns an error if the message cannot be parsed, recipient resolution
    /// fails, or the storage operation fails.
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
        if recipients.is_empty() {
            return Err(VivariumError::Message(
                "no recipient address resolves to a known local identity".into(),
            ));
        }
        let role = canonical_local_role(folder)?;
        let requests: Vec<MessageIngestRequest> = recipients
            .iter()
            .map(|recipient| MessageIngestRequest {
                account: recipient.clone(),
                local_role: role.clone(),
                read_state: false,
                starred: false,
                message_id_hint: None,
                seed_hint: format!("raw-delivery\0{recipient}\0{}", data.len()),
                remote: None,
            })
            .collect();
        let subject = parsed.subject().unwrap_or_default().to_string();
        let mut storage = self.storage()?;
        let stored = storage.deliver_raw_batch(
            &requests,
            data,
            "mail deliver",
            "delivered",
            &role,
            &subject,
        )?;
        stored
            .into_iter()
            .zip(requests.iter())
            .map(|(msg, req)| {
                Ok(DeliveredMessage {
                    identity: req.account.clone(),
                    handle: storage.display_handle(&msg.message_id)?,
                })
            })
            .collect()
    }

    /// List messages for a given identity and canonical role.
    ///
    /// # Errors
    /// Returns an error if the identity cannot be resolved or the storage
    /// operation fails.
    pub fn list(
        &self,
        identity: &str,
        role: &str,
    ) -> Result<Vec<StoredMessageView>, VivariumError> {
        let identity = self.resolve_identity(identity)?;
        let names = self.identity_names(&identity);
        let role = canonical_local_role(role)?;
        let storage = self.storage()?;
        Ok(storage
            .list_messages_by_role(&role)?
            .into_iter()
            .filter(|message| names.contains(&message.account))
            .collect())
    }

    /// List messages for a given identity, role, and kind (e.g. `task`, `need`).
    ///
    /// # Errors
    /// Returns an error if the identity cannot be resolved, the role is invalid,
    /// or the storage operation fails.
    pub fn list_kind(
        &self,
        identity: &str,
        role: &str,
        kind: &str,
    ) -> Result<Vec<StoredMessageView>, VivariumError> {
        let identity = self.resolve_identity(identity)?;
        let names = sorted_identity_names(self.identity_names(&identity));
        let role = canonical_local_role(role)?;
        let storage = self.storage()?;
        let role_messages =
            storage.list_messages_by_account_roles_scoped(&names, std::slice::from_ref(&role))?;
        if role_implies_kind(&role, kind) {
            return Ok(role_messages);
        }
        let mut messages = Vec::new();
        for message in role_messages {
            let data = storage.read_message(&message.message_id)?;
            let events = storage.list_mailspace_events(&message.message_id)?;
            if matches_kind(&message.local_role, &data, &events, kind) {
                messages.push(message);
            }
        }
        Ok(messages)
    }

    /// Search memos for an identity by query.
    ///
    /// Matches subject and body (unless `subject_only` is true). Search is
    /// case-insensitive. Only memos whose subject does not already match are
    /// parsed for body text, so the common case is cheap.
    ///
    /// # Errors
    /// Returns an error if the identity cannot be resolved, a storage
    /// operation fails, or a memo blob cannot be parsed.
    pub fn search_memos(
        &self,
        identity: &str,
        query: &str,
        subject_only: bool,
    ) -> Result<Vec<StoredMessageView>, VivariumError> {
        let query = query.to_lowercase();
        let memos = self.list_kind(identity, "memos", "memo")?;
        if subject_only {
            return Ok(memos
                .into_iter()
                .filter(|memo| memo.subject.to_lowercase().contains(&query))
                .collect());
        }
        let storage = self.storage()?;
        let mut matched = Vec::new();
        for memo in memos {
            if memo.subject.to_lowercase().contains(&query) {
                matched.push(memo);
                continue;
            }
            let data = storage.read_message(&memo.message_id)?;
            let extracted = crate::extract::extract_text(&data)?;
            if extracted.body_text.to_lowercase().contains(&query) {
                matched.push(memo);
            }
        }
        Ok(matched)
    }

    /// Move a task message to a new role (e.g. `done`, `tasks`). If `note` is
    /// provided, a reply message is also created.
    ///
    /// # Errors
    /// Returns an error if the identity or message cannot be resolved, the role
    /// is invalid, or the storage operation fails.
    pub fn move_task(
        &self,
        identity: &str,
        handle: &str,
        role: &str,
        note: Option<&str>,
    ) -> Result<String, VivariumError> {
        self.move_item(identity, handle, role, note, move_command("task", role))
    }

    /// Move a message to a new role with an optional note reply.
    ///
    /// # Errors
    /// Returns an error if the identity or message cannot be resolved, the role
    /// is invalid, note reply composition fails, or the storage operation fails.
    pub fn move_item(
        &self,
        identity: &str,
        handle: &str,
        role: &str,
        note: Option<&str>,
        command: &str,
    ) -> Result<String, VivariumError> {
        let identity = self.resolve_identity(identity)?;
        let names = sorted_identity_names(self.identity_names(&identity));
        let role = canonical_local_role(role)?;
        let mut storage = self.storage()?;
        let resolved = storage.resolve_message_token_for_accounts(handle, &names)?;
        let Some(before) = storage.message_by_id(&resolved)? else {
            return Err(VivariumError::Message(format!(
                "message not found: {handle}"
            )));
        };
        if !names.contains(&before.account) {
            return Err(VivariumError::Message(format!(
                "message not found for {identity}: {handle}"
            )));
        }
        // Stored messages keep the account name they were ingested under
        // even after a rename, so the storage-layer mutation must target
        // that historical account rather than the current canonical name.
        let account = before.account.clone();
        if let Some(note) = note {
            let reply = self.note_reply(&storage, &identity, &before, note)?;
            let event = MailspaceEventInput {
                command: command.into(),
                event_type: "moved".into(),
                actor_identity: Some(identity.clone()),
                account: account.clone(),
                message_id: before.message_id.clone(),
                content_id: before.content_id.clone(),
                from_role: Some(before.local_role.clone()),
                to_role: Some(role.clone()),
                from_identity: Some(identity.clone()),
                to_identity: Some(identity.clone()),
                subject: before.subject.clone(),
                note: Some(note.into()),
            };
            storage.move_message_with_reply(&MailspaceMoveWithReply {
                account: &account,
                message_id: &resolved,
                local_role: &role,
                event: &event,
                reply_requests: &reply.requests,
                reply_data: &reply.data,
                parent_content_id: &before.content_id,
            })?;
        } else {
            storage.move_message_to_role(&account, &resolved, &role)?;
            log_move_event(&storage, &identity, &role, &before, command, None)?;
        }
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

fn role_implies_kind(role: &str, kind: &str) -> bool {
    matches!(
        (role, kind),
        ("tasks", "task") | ("needs", "need") | ("wants", "want") | ("memos", "memo")
    )
}

fn sorted_identity_names(names: std::collections::HashSet<String>) -> Vec<String> {
    let mut names = names.into_iter().collect::<Vec<_>>();
    names.sort();
    names
}

fn move_command(kind: &str, role: &str) -> &'static str {
    match (kind, role) {
        ("task", "done") => "task done",
        ("task", _) => "task reopen",
        _ => "item move",
    }
}

pub(super) fn add_header(eml: &str, name: &str, value: &str) -> Result<String, VivariumError> {
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

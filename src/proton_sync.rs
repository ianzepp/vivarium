use chrono::{DateTime, Utc};
use sha2::{Digest, Sha256};

use crate::catalog::CatalogEntry;
use crate::config::{Account, StorageMode};
use crate::error::VivariumError;
use crate::proton_api::{ProtonApiClient, ProtonFullMessage, ProtonMessage, ProtonSessionStore};
use crate::proton_decrypt::ProtonBodyDecryptor;
use crate::storage::{MessageIngestRequest, Storage};
use crate::store::MailStore;
use crate::sync::{SyncResult, SyncWindow};

const PAGE_SIZE: usize = 150;

pub async fn sync_messages(
    account: &Account,
    store: &MailStore,
    limit: Option<usize>,
    window: SyncWindow,
) -> Result<SyncResult, VivariumError> {
    if !matches!(
        account.resolved_storage_mode(),
        StorageMode::Headers | StorageMode::Bodies
    ) {
        return Err(VivariumError::Config(format!(
            "account '{}' uses storage_mode = \"{}\"; direct Proton API sync currently supports storage_mode = \"headers\" or \"bodies\" only",
            account.name,
            account.resolved_storage_mode()
        )));
    }

    let session_store = ProtonSessionStore::new(store.root());
    let mut session = session_store.load()?;
    let client = ProtonApiClient::default();
    let mut storage = Storage::open(store.root())?;
    let mut result = SyncResult::default();
    let body_decryptor = body_decryptor(account, &client, &mut session, &session_store).await?;
    let mut page = 0;

    loop {
        let (refreshed, messages, total) = client.list_messages(&session, page, PAGE_SIZE).await?;
        session = refreshed;
        session_store.save(&session)?;
        if messages.is_empty() {
            break;
        }
        for message in messages {
            let mut ctx = SyncOneContext {
                account,
                client: &client,
                session_store: &session_store,
                session: &mut session,
                storage: &mut storage,
                body_decryptor: &body_decryptor,
                result: &mut result,
            };
            sync_one_message(&mut ctx, window, &message).await?;
            if limit.is_some_and(|limit| result.new >= limit) {
                return Ok(result);
            }
        }
        page += 1;
        if page.saturating_mul(PAGE_SIZE) >= total {
            break;
        }
    }

    Ok(result)
}

struct SyncOneContext<'a> {
    account: &'a Account,
    client: &'a ProtonApiClient,
    session_store: &'a ProtonSessionStore,
    session: &'a mut crate::proton_api::ProtonSession,
    storage: &'a mut Storage,
    body_decryptor: &'a Option<ProtonBodyDecryptor>,
    result: &'a mut SyncResult,
}

async fn sync_one_message(
    ctx: &mut SyncOneContext<'_>,
    window: SyncWindow,
    message: &ProtonMessage,
) -> Result<(), VivariumError> {
    if !message_in_window(message, window) {
        return Ok(());
    }
    let outcome = match ctx.body_decryptor {
        Some(decryptor) => {
            let (refreshed, full_message) =
                ctx.client.fetch_message(ctx.session, &message.id).await?;
            *ctx.session = refreshed;
            ctx.session_store.save(ctx.session)?;
            ingest_body(
                ctx.account,
                ctx.storage,
                decryptor,
                &full_message,
                ctx.result,
            )?
        }
        None => ingest_header(ctx.account, ctx.storage, message)?,
    };
    if let Some(entry) = outcome.entry {
        if outcome.is_new {
            ctx.result.new += 1;
        }
        ctx.result.cataloged_entries.push(entry);
    }
    Ok(())
}

async fn body_decryptor(
    account: &Account,
    client: &ProtonApiClient,
    session: &mut crate::proton_api::ProtonSession,
    session_store: &ProtonSessionStore,
) -> Result<Option<ProtonBodyDecryptor>, VivariumError> {
    if !matches!(account.resolved_storage_mode(), StorageMode::Bodies) {
        return Ok(None);
    }
    let password = account.resolve_secret().await?;
    let (refreshed, key_material) = client.key_material(session).await?;
    *session = refreshed;
    session_store.save(session)?;
    ProtonBodyDecryptor::new(&password, &key_material).map(Some)
}

struct IngestOutcome {
    entry: Option<CatalogEntry>,
    is_new: bool,
}

fn ingest_header(
    account: &Account,
    storage: &mut Storage,
    message: &ProtonMessage,
) -> Result<IngestOutcome, VivariumError> {
    let message_id = local_message_id(&message.id);
    let existed = storage.catalog_entry(&account.name, &message_id)?.is_some();
    let local_role = local_role(&message.label_ids);
    let stored = storage.ingest_message(
        &MessageIngestRequest {
            account: account.name.clone(),
            local_role,
            read_state: message.unread == 0,
            starred: false,
            message_id_hint: Some(message_id),
            seed_hint: format!("proton:{}", message.id),
            remote: None,
        },
        &header_bytes(message),
    )?;
    if existed {
        return Ok(IngestOutcome {
            entry: None,
            is_new: false,
        });
    }
    Ok(IngestOutcome {
        entry: storage.catalog_entry(&account.name, &stored.message_id)?,
        is_new: true,
    })
}

fn ingest_body(
    account: &Account,
    storage: &mut Storage,
    decryptor: &ProtonBodyDecryptor,
    message: &ProtonFullMessage,
    result: &mut SyncResult,
) -> Result<IngestOutcome, VivariumError> {
    let decrypted = decryptor.decrypt_body(&message.body);
    if decrypted.is_err() {
        result.decryption_errors += 1;
    }
    let bytes = decrypted
        .map(|body| body_bytes(message, &body))
        .unwrap_or_else(|_| decryption_failure_bytes(&message.metadata));
    let message_id = local_message_id(&message.metadata.id);
    let existed = storage.catalog_entry(&account.name, &message_id)?.is_some();
    let stored = storage.ingest_message(
        &MessageIngestRequest {
            account: account.name.clone(),
            local_role: local_role(&message.metadata.label_ids),
            read_state: message.metadata.unread == 0,
            starred: false,
            message_id_hint: Some(message_id),
            seed_hint: format!("proton:{}", message.metadata.id),
            remote: None,
        },
        &bytes,
    )?;
    Ok(IngestOutcome {
        entry: storage.catalog_entry(&account.name, &stored.message_id)?,
        is_new: !existed,
    })
}

fn message_in_window(message: &ProtonMessage, window: SyncWindow) -> bool {
    message
        .datetime()
        .is_none_or(|datetime| window.contains_datetime(datetime))
}

fn header_bytes(message: &ProtonMessage) -> Vec<u8> {
    let mut headers = Vec::new();
    push_header(&mut headers, "Date", &rfc2822_date(message.datetime()));
    push_header(&mut headers, "From", &message.sender.as_header_value());
    push_header(&mut headers, "To", &address_list(&message.to));
    push_header(&mut headers, "Cc", &address_list(&message.cc));
    push_header(&mut headers, "Bcc", &address_list(&message.bcc));
    push_header(&mut headers, "Subject", &message.subject);
    push_header(
        &mut headers,
        "Message-ID",
        &format!("<{}>", sanitize_header(&message.rfc_message_id())),
    );
    push_header(&mut headers, "X-Proton-Message-ID", &message.id);
    push_header(
        &mut headers,
        "X-Proton-Conversation-ID",
        &message.conversation_id,
    );
    push_header(
        &mut headers,
        "X-Proton-Label-IDs",
        &message.label_ids.join(","),
    );
    push_header(&mut headers, "X-Proton-Flags", &message.flags.to_string());
    push_header(
        &mut headers,
        "X-Proton-Num-Attachments",
        &message.num_attachments.to_string(),
    );
    push_header(&mut headers, "X-Proton-Size", &message.size.to_string());
    headers.push(String::new());
    headers.push(String::new());
    headers.join("\r\n").into_bytes()
}

fn body_bytes(message: &ProtonFullMessage, body: &[u8]) -> Vec<u8> {
    let mut bytes = if message.header.trim().is_empty() {
        header_bytes(&message.metadata)
    } else {
        normalize_header_block(&message.header).into_bytes()
    };
    bytes.extend_from_slice(body);
    bytes
}

fn decryption_failure_bytes(message: &ProtonMessage) -> Vec<u8> {
    let mut headers = String::from_utf8(header_bytes(message)).unwrap_or_default();
    let marker = "X-Vivarium-Proton-Decryption-Error: true\r\n";
    if let Some(index) = headers.find("\r\n\r\n") {
        headers.insert_str(index + 2, marker);
    } else {
        headers.push_str(marker);
        headers.push_str("\r\n");
    }
    headers.into_bytes()
}

fn normalize_header_block(header: &str) -> String {
    let mut header = header.replace("\r\n", "\n").replace('\r', "\n");
    while header.ends_with('\n') {
        header.pop();
    }
    format!("{}\r\n\r\n", header.replace('\n', "\r\n"))
}

fn push_header(headers: &mut Vec<String>, name: &str, value: &str) {
    if value.is_empty() {
        return;
    }
    headers.push(format!("{name}: {}", sanitize_header(value)));
}

fn sanitize_header(value: &str) -> String {
    value.replace(['\r', '\n'], " ").trim().to_string()
}

fn address_list(addresses: &[crate::proton_api::ProtonAddress]) -> String {
    addresses
        .iter()
        .map(|address| address.as_header_value())
        .filter(|value| !value.is_empty())
        .collect::<Vec<_>>()
        .join(", ")
}

fn rfc2822_date(date: Option<DateTime<Utc>>) -> String {
    date.map(|date| date.to_rfc2822()).unwrap_or_default()
}

fn local_role(label_ids: &[String]) -> String {
    if contains_label(label_ids, "0") {
        "inbox"
    } else if contains_label(label_ids, "2") {
        "sent"
    } else if contains_label(label_ids, "1") {
        "drafts"
    } else if contains_label(label_ids, "3") {
        "trash"
    } else {
        "archive"
    }
    .into()
}

fn contains_label(label_ids: &[String], label: &str) -> bool {
    label_ids.iter().any(|id| id == label)
}

fn local_message_id(proton_id: &str) -> String {
    let digest = Sha256::digest(proton_id.as_bytes());
    format!("proton-{}", hex::encode(&digest[..8]))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::proton_api::ProtonAddress;

    #[test]
    fn maps_system_labels_to_local_roles() {
        assert_eq!(local_role(&["0".into()]), "inbox");
        assert_eq!(local_role(&["2".into()]), "sent");
        assert_eq!(local_role(&["1".into()]), "drafts");
        assert_eq!(local_role(&["3".into()]), "trash");
        assert_eq!(local_role(&["5".into()]), "archive");
    }

    #[test]
    fn header_bytes_redact_body_and_include_metadata() {
        let message = test_message();
        let header = String::from_utf8(header_bytes(&message)).unwrap();

        assert!(header.contains("Subject: hello"));
        assert!(header.contains("Message-ID: <external@example.com>"));
        assert!(header.contains("X-Proton-Message-ID: proton-id"));
        assert!(header.contains("X-Proton-Num-Attachments: 2"));
        assert!(header.ends_with("\r\n\r\n"));
    }

    #[test]
    fn body_bytes_normalize_header_and_append_cleartext() {
        let message = ProtonFullMessage {
            metadata: test_message(),
            header: "Subject: hello\nContent-Type: text/plain\n\n".into(),
            body: String::new(),
            mime_type: "text/plain".into(),
        };
        let bytes = body_bytes(&message, b"clear body");

        assert_eq!(
            String::from_utf8(bytes).unwrap(),
            "Subject: hello\r\nContent-Type: text/plain\r\n\r\nclear body"
        );
    }

    #[test]
    fn decryption_failure_bytes_record_local_marker() {
        let bytes = decryption_failure_bytes(&test_message());
        let message = String::from_utf8(bytes).unwrap();

        assert!(message.contains("X-Vivarium-Proton-Decryption-Error: true\r\n"));
        assert!(message.ends_with("\r\n\r\n"));
    }

    fn test_message() -> ProtonMessage {
        ProtonMessage {
            id: "proton-id".into(),
            conversation_id: "conversation-id".into(),
            external_id: "external@example.com".into(),
            subject: "hello".into(),
            time: 1_778_205_000,
            size: 123,
            flags: 4,
            unread: 0,
            num_attachments: 2,
            sender: ProtonAddress {
                name: "Sender".into(),
                address: "sender@example.com".into(),
            },
            to: vec![ProtonAddress {
                name: String::new(),
                address: "to@example.com".into(),
            }],
            cc: Vec::new(),
            bcc: Vec::new(),
            label_ids: vec!["0".into(), "5".into()],
        }
    }
}

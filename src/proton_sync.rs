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

mod cache;
#[cfg(test)]
mod tests;

use cache::ProtonRawMessageCache;

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
    let raw_cache = ProtonRawMessageCache::new(store.root());
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
                raw_cache: &raw_cache,
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
    raw_cache: &'a ProtonRawMessageCache,
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
            let full_message = fetch_or_load_full_message(ctx, &message.id).await?;
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

async fn fetch_or_load_full_message(
    ctx: &mut SyncOneContext<'_>,
    message_id: &str,
) -> Result<ProtonFullMessage, VivariumError> {
    if let Some(message) = ctx.raw_cache.load(message_id)? {
        return Ok(message);
    }
    let (refreshed, full_message) = ctx.client.fetch_message(ctx.session, message_id).await?;
    *ctx.session = refreshed;
    ctx.session_store.save(ctx.session)?;
    ctx.raw_cache.store(&full_message)?;
    Ok(full_message)
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

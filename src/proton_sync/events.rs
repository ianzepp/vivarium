use super::{
    ProtonRawMessageCache, body_decryptor, direct_sync_storage_supported, ingest_body,
    ingest_header, local_message_id,
};
use crate::config::Account;
use crate::error::VivariumError;
use crate::proton_api::{ProtonApiClient, ProtonFullMessage, ProtonSessionStore};
use crate::storage::Storage;
use crate::store::MailStore;
use crate::sync::SyncResult;

/// Syncs specific message IDs from the Proton API.
///
/// # Errors
/// Returns an error if the storage mode is unsupported, the session cannot be loaded,
/// or any API or storage call fails.
pub async fn sync_message_ids(
    account: &Account,
    store: &MailStore,
    message_ids: &[String],
) -> Result<SyncResult, VivariumError> {
    if !direct_sync_storage_supported(account) {
        return Err(VivariumError::Config(format!(
            "account '{}' uses storage_mode = \"{}\"; direct Proton API sync currently supports storage_mode = \"headers\", \"bodies\", or \"semantic\" only",
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

    for message_id in message_ids {
        let full_message = fetch_event_message(
            &client,
            &mut session,
            &session_store,
            &raw_cache,
            message_id,
        )
        .await?;
        let outcome = match &body_decryptor {
            Some(decryptor) => {
                ingest_body(account, &mut storage, decryptor, &full_message, &mut result)?
            }
            None => ingest_header(account, &mut storage, &full_message.metadata)?,
        };
        if let Some(entry) = outcome.entry {
            if outcome.is_new {
                result.new += 1;
            }
            result.cataloged_entries.push(entry);
        }
    }

    Ok(result)
}

/// Marks a message as deleted by its Proton ID.
///
/// # Errors
/// Returns an error if storage cannot be opened or the delete operation fails.
pub fn delete_message_id(
    account: &Account,
    store: &MailStore,
    proton_id: &str,
) -> Result<bool, VivariumError> {
    let mut storage = Storage::open(store.root())?;
    storage.mark_message_deleted(&account.name, &local_message_id(proton_id))
}

async fn fetch_event_message(
    client: &ProtonApiClient,
    session: &mut crate::proton_api::ProtonSession,
    session_store: &ProtonSessionStore,
    raw_cache: &ProtonRawMessageCache,
    message_id: &str,
) -> Result<ProtonFullMessage, VivariumError> {
    if let Some(message) = raw_cache.load(message_id)? {
        return Ok(message);
    }
    let (refreshed, full_message) = client.fetch_message(session, message_id).await?;
    *session = refreshed;
    session_store.save(session)?;
    raw_cache.store(&full_message)?;
    Ok(full_message)
}

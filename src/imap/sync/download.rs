use std::sync::Arc;

use futures::TryStreamExt;
use tokio::sync::Mutex;

use super::super::transport::{CHUNK_SIZE, ImapSession, RemoteMessage, WORKER_COUNT, connect};
use super::{store_message, uid_set_string};
use crate::catalog::CatalogEntry;
use crate::config::Account;
use crate::error::VivariumError;
use crate::store::MailStore;
use crate::sync::SyncResult;

#[derive(Clone, Copy)]
enum FetchMode {
    Headers,
    Full,
}

struct WorkerContext {
    worker_id: usize,
    account: Account,
    remote_folder: String,
    local_folder: String,
    store: Arc<MailStore>,
    reject_invalid_certs: bool,
    fetch_mode: FetchMode,
}

pub(super) async fn download_missing(
    account: &Account,
    store: &MailStore,
    remote_folder: &str,
    local_folder: &str,
    missing: Vec<RemoteMessage>,
    reject_invalid_certs: bool,
) -> Result<SyncResult, VivariumError> {
    let chunks = Arc::new(Mutex::new(message_chunks(missing)));
    let result = Arc::new(Mutex::new(SyncResult::default()));
    let store = Arc::new(store.clone());
    let fetch_mode = fetch_mode(account);
    let worker_count = WORKER_COUNT.min(chunks.lock().await.len());

    let mut handles = Vec::new();
    for worker_id in 0..worker_count {
        let context = WorkerContext {
            worker_id,
            account: account.clone(),
            remote_folder: remote_folder.to_string(),
            local_folder: local_folder.to_string(),
            store: Arc::clone(&store),
            reject_invalid_certs,
            fetch_mode,
        };
        handles.push(tokio::spawn(worker(
            context,
            Arc::clone(&chunks),
            Arc::clone(&result),
        )));
    }

    for handle in handles {
        match handle.await {
            Ok(Ok(())) => {}
            Ok(Err(e)) => return Err(e),
            Err(e) => return Err(VivariumError::Imap(format!("worker join failed: {e}"))),
        }
    }

    Ok(Arc::try_unwrap(result).unwrap().into_inner())
}

fn message_chunks(missing: Vec<RemoteMessage>) -> std::vec::IntoIter<Vec<RemoteMessage>> {
    missing
        .chunks(CHUNK_SIZE as usize)
        .map(|chunk| chunk.to_vec())
        .collect::<Vec<_>>()
        .into_iter()
}

fn fetch_mode(account: &Account) -> FetchMode {
    if account.stores_full_bodies() {
        FetchMode::Full
    } else {
        FetchMode::Headers
    }
}

async fn worker(
    context: WorkerContext,
    chunks: Arc<Mutex<std::vec::IntoIter<Vec<RemoteMessage>>>>,
    result: Arc<Mutex<SyncResult>>,
) -> Result<(), VivariumError> {
    let mut session = connect(&context.account, context.reject_invalid_certs).await?;
    session
        .select(&context.remote_folder)
        .await
        .map_err(|e| VivariumError::Imap(format!("worker select failed: {e}")))?;

    while let Some(uids) = next_chunk(&chunks).await {
        let uid_set = uid_set_string(&uids.iter().map(|msg| msg.uid).collect::<Vec<_>>());
        tracing::debug!(
            worker_id = context.worker_id,
            uids = uid_set,
            "fetching chunk"
        );

        let messages = fetch_messages(&mut session, &uid_set, context.fetch_mode).await?;
        let entries = process_messages(&context, &uids, &messages)?;
        if !entries.is_empty() {
            let mut r = result.lock().await;
            r.new += entries.len();
            r.cataloged_entries.extend(entries);
        }
    }

    session.logout().await.ok();
    Ok(())
}

async fn next_chunk(
    chunks: &Mutex<std::vec::IntoIter<Vec<RemoteMessage>>>,
) -> Option<Vec<RemoteMessage>> {
    chunks.lock().await.next()
}

async fn fetch_messages(
    session: &mut ImapSession,
    uid_set: &str,
    mode: FetchMode,
) -> Result<Vec<async_imap::types::Fetch>, VivariumError> {
    let fetch_items = match mode {
        FetchMode::Headers => "BODY.PEEK[HEADER]",
        FetchMode::Full => "BODY[]",
    };
    let fetches = session
        .uid_fetch(uid_set, fetch_items)
        .await
        .map_err(|e| VivariumError::Imap(format!("fetch failed: {e}")))?;
    fetches
        .try_collect()
        .await
        .map_err(|e| VivariumError::Imap(format!("fetch stream failed: {e}")))
}

fn process_messages(
    context: &WorkerContext,
    chunk: &[RemoteMessage],
    messages: &[async_imap::types::Fetch],
) -> Result<Vec<CatalogEntry>, VivariumError> {
    let mut entries = Vec::new();
    for fetch in messages {
        let uid = fetch.uid.unwrap_or(0);
        let Some(body) = fetched_bytes(fetch, context.fetch_mode) else {
            continue;
        };
        let Some(remote) = chunk.iter().find(|message| message.uid == uid) else {
            continue;
        };
        entries.push(store_message(
            &context.account,
            &context.store,
            &context.remote_folder,
            &context.local_folder,
            body,
            remote,
        )?);
    }
    Ok(entries)
}

fn fetched_bytes(fetch: &async_imap::types::Fetch, mode: FetchMode) -> Option<&[u8]> {
    match mode {
        FetchMode::Headers => fetch.header().or_else(|| fetch.body()),
        FetchMode::Full => fetch.body(),
    }
}

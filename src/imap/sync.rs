use std::sync::Arc;
use std::time::Duration;

use async_imap::extensions::idle::IdleResponse;
use futures::TryStreamExt;
use tokio::sync::Mutex;

use super::query::fetch_remote_messages;
use super::transport::{CHUNK_SIZE, ImapSession, RemoteMessage, WORKER_COUNT, connect};
use crate::config::{Account, Provider};
use crate::error::VivariumError;
use crate::message::message_id_from_bytes;
use crate::store::MailStore;
use crate::sync::{SyncResult, SyncWindow};

struct WorkerContext {
    worker_id: usize,
    account: Account,
    remote_folder: String,
    local_folder: String,
    store: Arc<MailStore>,
    reject_invalid_certs: bool,
}

#[derive(Clone, Copy)]
struct FolderPlan<'a> {
    remote_folder: &'a str,
    local_folder: &'static str,
    dedupe_scope: DedupeScope,
}

#[derive(Clone, Copy)]
enum DedupeScope {
    LocalFolder,
    AllFolders,
}

/// Sync messages from the account's IMAP server into the local store.
pub async fn sync_messages(
    account: &Account,
    store: &MailStore,
    reject_invalid_certs: bool,
    limit: Option<usize>,
    window: SyncWindow,
) -> Result<SyncResult, VivariumError> {
    let mut result = SyncResult::default();
    let mut remaining = limit;

    for plan in sync_folders(account) {
        let r = sync_folder(
            account,
            store,
            plan.remote_folder,
            plan.local_folder,
            plan.dedupe_scope,
            reject_invalid_certs,
            remaining,
            window,
        )
        .await?;
        result.new += r.new;
        if let Some(value) = remaining.as_mut() {
            *value = value.saturating_sub(r.new);
            if *value == 0 {
                break;
            }
        }
    }

    Ok(result)
}

fn sync_folders(account: &Account) -> Vec<FolderPlan<'_>> {
    let mut folders = vec![
        FolderPlan {
            remote_folder: "INBOX",
            local_folder: "inbox",
            dedupe_scope: DedupeScope::LocalFolder,
        },
        FolderPlan {
            remote_folder: account.sent_folder(),
            local_folder: "sent",
            dedupe_scope: DedupeScope::LocalFolder,
        },
    ];

    if matches!(account.provider, Provider::Gmail | Provider::Protonmail) {
        folders.push(FolderPlan {
            remote_folder: account.all_mail_folder(),
            local_folder: "archive",
            dedupe_scope: DedupeScope::AllFolders,
        });
    }

    folders
}

/// Compare remote messages against local files and return missing ones.
fn find_missing(
    remote_messages: &[RemoteMessage],
    store: &MailStore,
    local_folder: &str,
    dedupe_scope: DedupeScope,
) -> Result<Vec<RemoteMessage>, VivariumError> {
    let local_sizes = store.local_sizes(local_folder)?;
    let rfc_index = match dedupe_scope {
        DedupeScope::LocalFolder => store.build_rfc_index(local_folder)?,
        DedupeScope::AllFolders => {
            let mut index = store.build_rfc_index(local_folder)?;
            for folder in ["inbox", "sent", "drafts"] {
                index.extend(store.build_rfc_index(folder)?);
            }
            index
        }
    };
    let mut missing: Vec<RemoteMessage> = Vec::new();

    for remote in remote_messages {
        if let Some(ref rfc_message_id) = remote.rfc_message_id
            && store.rfc_index_contains(&rfc_index, rfc_message_id)
        {
            continue;
        }

        let msg_id = format!("{local_folder}-{}", remote.uid);
        if let Some(local_size) = local_sizes.get(&msg_id)
            && *local_size == remote.size
        {
            continue;
        }
        missing.push(remote.clone());
    }

    Ok(missing)
}

/// Download missing messages using a chunked worker pool.
async fn download_missing(
    account: &Account,
    store: &MailStore,
    remote_folder: &str,
    local_folder: &str,
    missing: Vec<RemoteMessage>,
    reject_invalid_certs: bool,
) -> Result<SyncResult, VivariumError> {
    let chunks: Vec<Vec<RemoteMessage>> = missing
        .chunks(CHUNK_SIZE as usize)
        .map(|c| c.to_vec())
        .collect();
    let chunks = Arc::new(Mutex::new(chunks.into_iter()));
    let result = Arc::new(Mutex::new(SyncResult::default()));
    let store = Arc::new(store.clone());

    let mut handles = Vec::new();
    let worker_count = WORKER_COUNT.min(missing.len().div_ceil(CHUNK_SIZE as usize));

    for worker_id in 0..worker_count {
        let chunks = Arc::clone(&chunks);
        let result = Arc::clone(&result);
        let store = Arc::clone(&store);
        let account = account.clone();
        let local_folder = local_folder.to_string();
        let remote_folder = remote_folder.to_string();

        let handle = tokio::spawn(async move {
            worker(
                WorkerContext {
                    worker_id,
                    account,
                    remote_folder,
                    local_folder,
                    store,
                    reject_invalid_certs,
                },
                &chunks,
                &result,
            )
            .await
        });
        handles.push(handle);
    }

    for handle in handles {
        match handle.await {
            Ok(Ok(())) => {}
            Ok(Err(e)) => return Err(e),
            Err(e) => return Err(VivariumError::Imap(format!("worker join failed: {e}"))),
        }
    }

    let result = Arc::try_unwrap(result).unwrap().into_inner();
    Ok(result)
}

/// Sync messages from the account's IMAP server into the local store.
async fn sync_folder(
    account: &Account,
    store: &MailStore,
    remote_folder: &str,
    local_folder: &str,
    dedupe_scope: DedupeScope,
    reject_invalid_certs: bool,
    limit: Option<usize>,
    window: SyncWindow,
) -> Result<SyncResult, VivariumError> {
    let remote_messages =
        fetch_remote_messages(account, remote_folder, reject_invalid_certs, window).await?;
    if remote_messages.is_empty() {
        return Ok(SyncResult::default());
    }

    let mut missing = find_missing(&remote_messages, store, local_folder, dedupe_scope)?;
    if missing.is_empty() {
        tracing::info!(
            folder = remote_folder,
            total = remote_messages.len(),
            "all messages up to date"
        );
        return Ok(SyncResult::default());
    }
    let total_missing = missing.len();
    if let Some(limit) = limit {
        missing.truncate(limit);
        if missing.is_empty() {
            return Ok(SyncResult::default());
        }
    }

    tracing::info!(
        folder = remote_folder,
        total = remote_messages.len(),
        missing = total_missing,
        downloading = missing.len(),
        "downloading new messages"
    );

    download_missing(
        account,
        store,
        remote_folder,
        local_folder,
        missing,
        reject_invalid_certs,
    )
    .await
}

/// Store a single parsed message in the local Maildir and update the index.
fn store_message(
    store: &MailStore,
    local_folder: &str,
    body: &[u8],
    uid: u32,
) -> Result<(), VivariumError> {
    let message_id = format!("{local_folder}-{uid}");
    let subdir = if local_folder == "inbox" {
        "new"
    } else {
        "cur"
    };
    store.store_message_in(local_folder, subdir, &message_id, body)?;
    if let Some(rfc_message_id) = message_id_from_bytes(body) {
        let size = u64::try_from(body.len()).unwrap_or(u64::MAX);
        store.write_message_index(local_folder, &rfc_message_id, uid, size)?;
    }
    Ok(())
}

/// Worker: grab chunks from the queue, open a connection, fetch messages.
async fn worker(
    context: WorkerContext,
    chunks: &Mutex<std::vec::IntoIter<Vec<RemoteMessage>>>,
    result: &Mutex<SyncResult>,
) -> Result<(), VivariumError> {
    let mut session = connect(&context.account, context.reject_invalid_certs).await?;
    session
        .select(&context.remote_folder)
        .await
        .map_err(|e| VivariumError::Imap(format!("worker select failed: {e}")))?;

    loop {
        let uids = {
            let mut iter = chunks.lock().await;
            match iter.next() {
                Some(c) => c,
                None => break,
            }
        };

        let uid_set = uid_set_string(&uids.iter().map(|msg| msg.uid).collect::<Vec<_>>());
        tracing::debug!(
            worker_id = context.worker_id,
            uids = uid_set,
            "fetching chunk"
        );

        let messages = fetch_messages(&mut session, &uid_set).await?;
        let new_count = process_messages(&context, &messages)?;
        if new_count > 0 {
            let mut r = result.lock().await;
            r.new += new_count;
        }
    }

    session.logout().await.ok();
    Ok(())
}

/// Fetch messages by UID set from the IMAP server.
async fn fetch_messages(
    session: &mut ImapSession,
    uid_set: &str,
) -> Result<Vec<async_imap::types::Fetch>, VivariumError> {
    let fetches = session
        .uid_fetch(uid_set, "BODY[]")
        .await
        .map_err(|e| VivariumError::Imap(format!("fetch failed: {e}")))?;
    fetches
        .try_collect()
        .await
        .map_err(|e| VivariumError::Imap(format!("fetch stream failed: {e}")))
}

/// Parse and store a batch of fetched messages. Returns count of stored messages.
fn process_messages(
    context: &WorkerContext,
    messages: &[async_imap::types::Fetch],
) -> Result<usize, VivariumError> {
    let mut new_count = 0;
    for fetch in messages {
        let uid = fetch.uid.unwrap_or(0);
        let body = match fetch.body() {
            Some(b) => b,
            None => continue,
        };
        store_message(&context.store, &context.local_folder, body, uid)?;
        new_count += 1;
    }
    Ok(new_count)
}

/// Build a UID set string like "1,2,3,5,6,7" from a list of UIDs.
fn uid_set_string(uids: &[u32]) -> String {
    uids.iter()
        .map(|u| u.to_string())
        .collect::<Vec<_>>()
        .join(",")
}

pub async fn idle(account: &Account, reject_invalid_certs: bool) -> Result<(), VivariumError> {
    let mut session = connect(account, reject_invalid_certs).await?;
    session
        .select("INBOX")
        .await
        .map_err(|e| VivariumError::Imap(format!("idle select INBOX failed: {e}")))?;

    let mut handle = session.idle();
    handle
        .init()
        .await
        .map_err(|e| VivariumError::Imap(format!("IDLE init failed: {e}")))?;

    let (wait, _interrupt) = handle.wait_with_timeout(Duration::from_secs(29 * 60));
    match wait
        .await
        .map_err(|e| VivariumError::Imap(format!("IDLE wait failed: {e}")))?
    {
        IdleResponse::NewData(response) => {
            tracing::debug!(response = ?response.parsed(), "IMAP IDLE notification");
        }
        IdleResponse::Timeout => {
            tracing::debug!("IMAP IDLE timed out; refreshing connection");
        }
        IdleResponse::ManualInterrupt => {
            tracing::debug!("IMAP IDLE interrupted");
        }
    }

    let mut session = handle
        .done()
        .await
        .map_err(|e| VivariumError::Imap(format!("IDLE done failed: {e}")))?;
    session.logout().await.ok();
    Ok(())
}

#[cfg(test)]
mod tests;

use self::download::download_missing;
use super::query::fetch_remote_messages;
use super::transport::RemoteMessage;
use crate::catalog::CatalogEntry;
use crate::config::{Account, Provider, StorageMode};
use crate::error::VivariumError;
use crate::storage::{MessageIngestRequest, RemoteBindingInput, Storage};
use crate::store::MailStore;
use crate::sync::{SyncResult, SyncWindow};

mod download;

struct FolderPlan {
    remote_folder: String,
    local_folder: &'static str,
    dedupe_scope: DedupeScope,
}

struct SyncFolderRequest<'a> {
    account: &'a Account,
    store: &'a MailStore,
    plan: FolderPlan,
    reject_invalid_certs: bool,
    limit: Option<usize>,
    window: SyncWindow,
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
    all: bool,
) -> Result<SyncResult, VivariumError> {
    if matches!(account.provider, Provider::ProtonApi) {
        return Err(VivariumError::Config(format!(
            "account '{}' uses provider = \"proton-api\"; IMAP sync is not available for direct Proton API accounts yet",
            account.name
        )));
    }

    if matches!(account.resolved_storage_mode(), StorageMode::Proxy) {
        return Err(VivariumError::Config(format!(
            "account '{}' uses storage_mode = \"proxy\"; sync requires headers, bodies, or semantic storage",
            account.name
        )));
    }

    let mut result = SyncResult::default();
    let mut remaining = limit;

    for plan in sync_folders(account, all) {
        let r = sync_folder(SyncFolderRequest {
            account,
            store,
            plan,
            reject_invalid_certs,
            limit: remaining,
            window,
        })
        .await?;
        result.new += r.new;
        result.cataloged_entries.extend(r.cataloged_entries);
        if let Some(value) = remaining.as_mut() {
            *value = value.saturating_sub(r.new);
            if *value == 0 {
                break;
            }
        }
    }

    Ok(result)
}

fn sync_folders(account: &Account, all: bool) -> Vec<FolderPlan> {
    let mut folders = vec![
        FolderPlan {
            remote_folder: account.inbox_folder(),
            local_folder: "inbox",
            dedupe_scope: DedupeScope::LocalFolder,
        },
        FolderPlan {
            remote_folder: account.sent_folder(),
            local_folder: "sent",
            dedupe_scope: DedupeScope::LocalFolder,
        },
    ];

    if all && matches!(account.provider, Provider::Gmail | Provider::Protonmail) {
        folders.push(FolderPlan {
            remote_folder: account.all_mail_folder().into(),
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

/// Sync messages from the account's IMAP server into the local store.
async fn sync_folder(request: SyncFolderRequest<'_>) -> Result<SyncResult, VivariumError> {
    let remote_folder = request.plan.remote_folder.as_str();
    let local_folder = request.plan.local_folder;
    let remote_messages = fetch_remote_messages(
        request.account,
        remote_folder,
        request.reject_invalid_certs,
        request.window,
    )
    .await?;
    if remote_messages.is_empty() {
        return Ok(SyncResult::default());
    }
    refresh_remote_flags(
        request.account,
        request.store,
        remote_folder,
        &remote_messages,
    )?;
    let mut missing = find_missing(
        &remote_messages,
        request.store,
        local_folder,
        request.plan.dedupe_scope,
    )?;
    if missing.is_empty() {
        tracing::info!(
            folder = remote_folder,
            total = remote_messages.len(),
            "all messages up to date"
        );
        return Ok(SyncResult::default());
    }
    let total_missing = missing.len();
    truncate_missing(&mut missing, request.limit);
    if missing.is_empty() {
        return Ok(SyncResult::default());
    }
    tracing::info!(
        folder = remote_folder,
        total = remote_messages.len(),
        missing = total_missing,
        downloading = missing.len(),
        "downloading new messages"
    );

    download_missing(
        request.account,
        request.store,
        remote_folder,
        local_folder,
        missing,
        request.reject_invalid_certs,
    )
    .await
}

fn truncate_missing(missing: &mut Vec<RemoteMessage>, limit: Option<usize>) {
    if let Some(limit) = limit {
        missing.truncate(limit);
    }
}

fn refresh_remote_flags(
    account: &Account,
    store: &MailStore,
    remote_folder: &str,
    remote_messages: &[RemoteMessage],
) -> Result<(), VivariumError> {
    let mut storage = Storage::open(store.root())?;
    for remote in remote_messages {
        let Some(uidvalidity) = remote.uidvalidity else {
            continue;
        };
        storage.update_remote_flags(
            &account.name,
            remote_folder,
            uidvalidity,
            remote.uid,
            remote.read_state,
            remote.starred,
        )?;
    }
    Ok(())
}

/// Store a single parsed message in hash-addressed storage.
fn store_message(
    account: &Account,
    store: &MailStore,
    remote_folder: &str,
    local_folder: &str,
    body: &[u8],
    remote: &RemoteMessage,
) -> Result<CatalogEntry, VivariumError> {
    let uid = remote.uid;
    let mut storage = Storage::open(store.root())?;
    let stored = storage.ingest_message(
        &MessageIngestRequest {
            account: account.name.clone(),
            local_role: local_folder.to_string(),
            read_state: remote.read_state,
            starred: remote.starred,
            message_id_hint: None,
            seed_hint: format!("remote_uid:{uid}"),
            remote: remote.uidvalidity.map(|uidvalidity| RemoteBindingInput {
                account: account.name.clone(),
                provider: account.provider.to_string(),
                remote_mailbox: remote_folder.to_string(),
                remote_uid: uid,
                remote_uidvalidity: uidvalidity,
            }),
        },
        body,
    )?;
    storage
        .catalog_entry(&account.name, &stored.message_id)?
        .ok_or_else(|| {
            VivariumError::Other(format!(
                "stored message missing from catalog view: {}",
                stored.message_id
            ))
        })
}

fn uid_set_string(uids: &[u32]) -> String {
    uids.iter()
        .map(|u| u.to_string())
        .collect::<Vec<_>>()
        .join(",")
}

#[cfg(test)]
mod tests;

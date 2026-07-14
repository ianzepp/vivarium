use std::cell::RefCell;
use std::collections::HashMap;
use std::fs::{self, OpenOptions};
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::time::Duration;

use chrono::Utc;
use rusqlite::{Connection, OptionalExtension, params};
use sha2::{Digest, Sha256};

use crate::catalog::{CatalogEntry, RemoteIdentity};
use crate::error::VivariumError;
use crate::store::secure_create_dir_all;

mod events;
mod handles;
mod ingest;
mod item_metadata;
mod links;
mod metadata;
mod mutate;
mod query;
mod schema;
#[cfg(test)]
mod tests;

pub use links::MailspaceLink;
use metadata::parse_metadata;
pub use mutate::MailspaceMoveWithReply;
use schema::{ensure_schema, message_query};

const INTERNAL_DIR: &str = ".vivarium";
const STORAGE_DB_FILENAME: &str = "storage.sqlite";
const MAILSPACE_DB_FILENAME: &str = "mail.sqlite";
const BLOBS_DIR: &str = "blobs";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StorageImportResult {
    pub imported_messages: usize,
    pub imported_blobs: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredMessage {
    pub message_id: String,
    pub content_id: String,
    pub blob_relpath: String,
    pub created_blob: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MessageIngestRequest {
    pub account: String,
    pub local_role: String,
    pub read_state: bool,
    pub starred: bool,
    pub message_id_hint: Option<String>,
    pub seed_hint: String,
    pub remote: Option<RemoteBindingInput>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemoteBindingInput {
    pub account: String,
    pub provider: String,
    pub remote_mailbox: String,
    pub remote_uid: u32,
    pub remote_uidvalidity: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredMessageView {
    pub handle: String,
    pub message_id: String,
    pub account: String,
    pub content_id: String,
    pub blob_relpath: String,
    pub byte_size: u64,
    pub local_role: String,
    pub read_state: bool,
    pub starred: bool,
    pub date: String,
    pub from_addr: String,
    pub to_addr: String,
    pub cc_addr: String,
    pub bcc_addr: String,
    pub subject: String,
    pub normalized_message_id: Option<String>,
    pub remote: Option<RemoteBindingInput>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MailspaceEventInput {
    pub command: String,
    pub event_type: String,
    pub actor_identity: Option<String>,
    pub account: String,
    pub message_id: String,
    pub content_id: String,
    pub from_role: Option<String>,
    pub to_role: Option<String>,
    pub from_identity: Option<String>,
    pub to_identity: Option<String>,
    pub subject: String,
    pub note: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct MailspaceEvent {
    pub event_id: i64,
    pub occurred_at: String,
    pub command: String,
    pub event_type: String,
    pub actor_identity: Option<String>,
    pub account: String,
    pub message_id: String,
    pub content_id: String,
    pub from_role: Option<String>,
    pub to_role: Option<String>,
    pub from_identity: Option<String>,
    pub to_identity: Option<String>,
    pub subject: String,
    pub note: Option<String>,
}

pub struct Storage {
    mail_root: PathBuf,
    conn: Connection,
    /// Cached short-handle map, cleared on any write.
    handle_cache: RefCell<Option<HashMap<String, String>>>,
}

impl Storage {
    pub fn open(mail_root: &Path) -> Result<Self, VivariumError> {
        let internal_dir = mail_root.join(INTERNAL_DIR);
        secure_create_dir_all(&internal_dir)
            .map_err(|e| VivariumError::Other(format!("failed to create storage dir: {e}")))?;
        Self::open_with_db(mail_root, &internal_dir.join(STORAGE_DB_FILENAME))
    }

    pub fn open_mailspace(mailspace_dir: &Path) -> Result<Self, VivariumError> {
        secure_create_dir_all(mailspace_dir)
            .map_err(|e| VivariumError::Other(format!("failed to create mailspace dir: {e}")))?;
        Self::open_with_db(mailspace_dir, &mailspace_dir.join(MAILSPACE_DB_FILENAME))
    }

    fn open_with_db(mail_root: &Path, db_path: &Path) -> Result<Self, VivariumError> {
        secure_create_dir_all(&mail_root.join(BLOBS_DIR))
            .map_err(|e| VivariumError::Other(format!("failed to create blob dir: {e}")))?;

        let conn = Connection::open(db_path)
            .map_err(|e| VivariumError::Other(format!("failed to open storage database: {e}")))?;
        conn.busy_timeout(Duration::from_secs(5))
            .map_err(|e| VivariumError::Other(format!("failed to set SQLite timeout: {e}")))?;
        conn.pragma_update(None, "journal_mode", "WAL")
            .map_err(|e| VivariumError::Other(format!("failed to set WAL journal mode: {e}")))?;
        #[cfg(unix)]
        fs::set_permissions(db_path, fs::Permissions::from_mode(0o600))?;

        ensure_schema(&conn)?;

        Ok(Self {
            mail_root: mail_root.to_path_buf(),
            conn,
            handle_cache: RefCell::new(None),
        })
    }

    /// Clear the cached short-handle map after any write that affects messages.
    fn invalidate_handle_cache(&self) {
        *self.handle_cache.borrow_mut() = None;
    }

    pub fn import_catalog_entries(
        &mut self,
        entries: &[CatalogEntry],
    ) -> Result<StorageImportResult, VivariumError> {
        let mut result = StorageImportResult {
            imported_messages: 0,
            imported_blobs: 0,
        };
        for entry in entries {
            let data = fs::read(&entry.blob_path)?;
            let stored = self.ingest_message(&request_from_catalog_entry(entry), &data)?;
            result.imported_messages += 1;
            if stored.created_blob {
                result.imported_blobs += 1;
            }
        }
        Ok(result)
    }
}

fn raw_stored_message_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<StoredMessageView> {
    let remote_account: Option<String> = row.get(15)?;
    let remote = if let Some(account) = remote_account {
        Some(RemoteBindingInput {
            account,
            provider: row.get(16)?,
            remote_mailbox: row.get(17)?,
            remote_uid: row.get(18)?,
            remote_uidvalidity: row.get(19)?,
        })
    } else {
        None
    };
    let message_id: String = row.get(0)?;
    Ok(StoredMessageView {
        handle: message_id.clone(),
        message_id,
        account: row.get(1)?,
        content_id: row.get(2)?,
        blob_relpath: row.get(3)?,
        byte_size: row.get::<_, i64>(4)? as u64,
        local_role: row.get(5)?,
        read_state: row.get::<_, i64>(6)? != 0,
        starred: row.get::<_, i64>(7)? != 0,
        date: row.get(8)?,
        from_addr: row.get(9)?,
        to_addr: row.get(10)?,
        cc_addr: row.get(11)?,
        bcc_addr: row.get(12)?,
        subject: row.get(13)?,
        normalized_message_id: row.get(14)?,
        remote,
    })
}

pub fn import_catalog_entries(
    mail_root: &Path,
    entries: &[CatalogEntry],
) -> Result<StorageImportResult, VivariumError> {
    let mut storage = Storage::open(mail_root)?;
    storage.import_catalog_entries(entries)
}

fn blob_relpath(content_id: &str) -> String {
    format!(
        "{}/{}/{}/{}.eml",
        BLOBS_DIR,
        &content_id[..2],
        &content_id[2..4],
        content_id
    )
}

fn write_blob_if_absent(path: &Path, data: &[u8]) -> Result<bool, VivariumError> {
    if path.exists() {
        return Ok(false);
    }
    let Some(parent) = path.parent() else {
        return Err(VivariumError::Other(format!(
            "blob path has no parent: {}",
            path.display()
        )));
    };
    secure_create_dir_all(parent)
        .map_err(|e| VivariumError::Other(format!("failed to create blob dir: {e}")))?;
    match OpenOptions::new().write(true).create_new(true).open(path) {
        Ok(mut file) => {
            use std::io::Write;
            file.write_all(data)?;
            file.sync_all()?;
            #[cfg(unix)]
            fs::set_permissions(path, fs::Permissions::from_mode(0o600))?;
            Ok(true)
        }
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => Ok(false),
        Err(e) => Err(VivariumError::Io(e)),
    }
}

#[cfg(test)]
fn local_role(folder: &str) -> String {
    match folder {
        "INBOX" | "Inbox" | "inbox" => "inbox".into(),
        "Archive" | "archive" => "archive".into(),
        "Trash" | "trash" => "trash".into(),
        "Sent" | "sent" => "sent".into(),
        "Drafts" | "drafts" => "drafts".into(),
        other => other.to_ascii_lowercase(),
    }
}

fn remote_bound_message_id(
    account: &str,
    local_role: &str,
    content_id: &str,
    remote: &RemoteBindingInput,
) -> String {
    let seed = format!(
        "remote\0{account}\0{local_role}\0{content_id}\0{}\0{}\0{}",
        remote.remote_mailbox, remote.remote_uidvalidity, remote.remote_uid
    );
    opaque_message_id(&seed)
}

fn fallback_message_id(request: &MessageIngestRequest, content_id: &str) -> String {
    let seed = format!(
        "local\0{}\0{}\0{}\0{}",
        request.account, request.local_role, request.seed_hint, content_id
    );
    opaque_message_id(&seed)
}

fn request_from_catalog_entry(entry: &CatalogEntry) -> MessageIngestRequest {
    MessageIngestRequest {
        account: entry.account.clone(),
        local_role: entry.local_role.clone(),
        read_state: entry.read_state,
        starred: entry.starred,
        message_id_hint: Some(entry.handle.clone()),
        seed_hint: entry.handle.clone(),
        remote: entry.remote.as_ref().map(remote_binding_from_catalog),
    }
}

fn remote_binding_from_catalog(remote: &RemoteIdentity) -> RemoteBindingInput {
    RemoteBindingInput {
        account: remote.account.clone(),
        provider: remote.provider.clone(),
        remote_mailbox: remote.remote_mailbox.clone(),
        remote_uid: remote.uid,
        remote_uidvalidity: remote.uidvalidity,
    }
}

impl Storage {
    fn catalog_entry_from_view(
        &self,
        message: StoredMessageView,
    ) -> Result<CatalogEntry, VivariumError> {
        let remote = message.remote.as_ref().map(|binding| RemoteIdentity {
            account: binding.account.clone(),
            provider: binding.provider.clone(),
            remote_mailbox: binding.remote_mailbox.clone(),
            local_folder: message.local_role.clone(),
            uid: binding.remote_uid,
            uidvalidity: binding.remote_uidvalidity,
            rfc_message_id: message.normalized_message_id.clone().unwrap_or_default(),
            size: message.byte_size,
            content_fingerprint: message.content_id.clone(),
        });
        Ok(CatalogEntry {
            handle: message.message_id.clone(),
            account: message.account,
            content_id: message.content_id,
            blob_path: self
                .mail_root
                .join(&message.blob_relpath)
                .to_string_lossy()
                .to_string(),
            local_role: message.local_role,
            read_state: message.read_state,
            starred: message.starred,
            date: message.date,
            from: message.from_addr,
            to: message.to_addr,
            cc: message.cc_addr,
            bcc: message.bcc_addr,
            subject: message.subject,
            rfc_message_id: message.normalized_message_id.unwrap_or_default(),
            remote,
        })
    }
}

fn opaque_message_id(seed: &str) -> String {
    let hash = sha256_hex(seed.as_bytes());
    format!("msg_{}", &hash[..24])
}

fn short_handle_map(message_ids: &[String]) -> HashMap<String, String> {
    let bases = message_ids
        .iter()
        .map(|message_id| (message_id.clone(), handle_basis(message_id).to_string()))
        .collect::<Vec<_>>();
    let mut map = HashMap::new();
    for (message_id, basis) in &bases {
        if !message_id.starts_with("msg_") {
            map.insert(message_id.clone(), message_id.clone());
            continue;
        }
        let min_len = usize::min(7, basis.len());
        let mut handle = basis.clone();
        for len in min_len..=basis.len() {
            let prefix = &basis[..len];
            let count = bases
                .iter()
                .filter(|(_, other)| other.starts_with(prefix))
                .count();
            if count == 1 {
                handle = prefix.to_string();
                break;
            }
        }
        map.insert(message_id.clone(), handle);
    }
    map
}

fn handle_basis(message_id: &str) -> &str {
    message_id.strip_prefix("msg_").unwrap_or(message_id)
}

fn sha256_hex(data: &[u8]) -> String {
    let hash = Sha256::digest(data);
    hex::encode(hash)
}

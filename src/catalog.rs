use std::fs;
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use hex;
use rusqlite::{Connection, params};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::error::VivariumError;
use crate::storage::{MessageIngestRequest, RemoteBindingInput, Storage};
use crate::store::secure_create_dir_all;

mod local;
mod remote;
#[cfg(test)]
mod tests;

pub use remote::{
    RemoteIdentity, RemoteIdentityAttachResult, RemoteIdentityCandidate, RemoteReferenceStatus,
    attach_remote_identities,
};

/// Catalog directory inside the mail root.
const CATALOG_DIR: &str = ".vivarium";

/// Source-of-truth storage database filename.
const STORAGE_DB_FILENAME: &str = "storage.sqlite";

/// Stable handle prefix length (hex chars of SHA-256).
const HANDLE_LENGTH: usize = 16;

/// A message row in the catalog.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CatalogEntry {
    pub handle: String,
    pub account: String,
    pub content_id: String,
    pub blob_path: String,
    pub local_role: String,
    pub read_state: bool,
    pub starred: bool,
    pub date: String,
    pub from: String,
    pub to: String,
    pub cc: String,
    pub bcc: String,
    pub subject: String,
    pub rfc_message_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub remote: Option<RemoteIdentity>,
}

/// The mail catalog backed by `SQLite`.
pub struct Catalog {
    mail_root: PathBuf,
    conn: Connection,
}

impl Catalog {
    /// Open or create the catalog at the given mail root.
    ///
    /// # Errors
    /// Returns an error if the storage or catalog database cannot be opened or created.
    pub fn open(mail_root: &Path) -> Result<Self, VivariumError> {
        Storage::open(mail_root)?;
        let catalog_dir = mail_root.join(CATALOG_DIR);
        secure_create_dir_all(&catalog_dir)
            .map_err(|e| VivariumError::Other(format!("failed to create catalog dir: {e}")))?;
        let catalog_path = catalog_dir.join(STORAGE_DB_FILENAME);
        let conn = Connection::open(&catalog_path).map_err(|e| {
            VivariumError::Other(format!(
                "failed to open storage-backed catalog database: {e}"
            ))
        })?;
        conn.pragma_update(None, "foreign_keys", "ON")
            .map_err(|e| {
                VivariumError::Other(format!("failed to enable catalog foreign keys: {e}"))
            })?;
        #[cfg(unix)]
        fs::set_permissions(&catalog_path, fs::Permissions::from_mode(0o600))?;

        Ok(Self {
            mail_root: mail_root.to_path_buf(),
            conn,
        })
    }

    /// `SQLite` autocommit persists writes; this keeps the old internal API shape.
    fn flush() {}

    /// Insert or update a single message in the catalog.
    ///
    /// # Errors
    /// Returns an error if the underlying storage operation fails.
    pub fn upsert(&mut self, entry: &CatalogEntry) -> Result<(), VivariumError> {
        let data = self.entry_bytes(entry);
        let request = MessageIngestRequest {
            account: entry.account.clone(),
            local_role: entry.local_role.clone(),
            read_state: entry.read_state,
            starred: entry.starred,
            message_id_hint: Some(entry.handle.clone()),
            seed_hint: entry.handle.clone(),
            remote: entry.remote.as_ref().map(remote_binding_input),
        };
        Storage::open(&self.mail_root)?.ingest_message(&request, &data)?;
        Self::flush();
        Ok(())
    }

    /// List all catalog entries for an account.
    ///
    /// # Errors
    /// Returns an error if the database query fails.
    pub fn list_messages(&self, account: &str) -> Result<Vec<CatalogEntry>, VivariumError> {
        let mut stmt = self
            .conn
            .prepare(&format!(
                "{} WHERE m.account = ?1 AND m.deleted_at IS NULL
                 ORDER BY md.date DESC, m.message_id",
                catalog_select_sql()
            ))
            .map_err(|e| VivariumError::Other(format!("failed to prepare catalog listing: {e}")))?;
        let rows = stmt
            .query_map(params![account], |row| self.catalog_entry_from_row(row))
            .map_err(|e| VivariumError::Other(format!("failed to list catalog rows: {e}")))?;
        rows.map(|row| {
            row.map_err(|e| VivariumError::Other(format!("failed to read catalog row: {e}")))
        })
        .collect()
    }

    /// Remove all entries for an account from the catalog.
    ///
    /// # Errors
    /// Returns an error if the database operation fails.
    pub fn remove_account(&mut self, account: &str) -> Result<(), VivariumError> {
        self.conn
            .execute("DELETE FROM messages WHERE account = ?1", params![account])
            .map_err(|e| VivariumError::Other(format!("failed to remove catalog account: {e}")))?;
        Self::flush();
        Ok(())
    }

    /// Count entries for an account.
    ///
    /// # Errors
    /// Returns an error if the database query fails.
    pub fn count_messages(&self, account: &str) -> Result<usize, VivariumError> {
        self.conn
            .query_row(
                "SELECT COUNT(*) FROM messages WHERE account = ?1 AND deleted_at IS NULL",
                params![account],
                |row| row.get(0),
            )
            .map_err(|e| VivariumError::Other(format!("failed to count catalog rows: {e}")))
    }

    fn catalog_entry_from_row(&self, row: &rusqlite::Row<'_>) -> rusqlite::Result<CatalogEntry> {
        let message_id: String = row.get(0)?;
        let account: String = row.get(1)?;
        let content_id: String = row.get(2)?;
        let blob_relpath: String = row.get(3)?;
        let local_role: String = row.get(5)?;
        let read_state = row.get::<_, i64>(6)? != 0;
        let starred = row.get::<_, i64>(7)? != 0;
        let normalized_message_id: Option<String> = row.get(14)?;
        let remote_account: Option<String> = row.get(15)?;
        let remote = remote_account.map(|remote_account| RemoteIdentity {
            account: remote_account,
            provider: row.get(16).unwrap_or_default(),
            remote_mailbox: row.get(17).unwrap_or_default(),
            local_folder: local_role.clone(),
            uid: row.get(18).unwrap_or_default(),
            uidvalidity: row.get(19).unwrap_or_default(),
            rfc_message_id: normalized_message_id.clone().unwrap_or_default(),
            size: row.get::<_, i64>(4).unwrap_or_default().unsigned_abs(),
            content_fingerprint: content_id.clone(),
        });

        Ok(CatalogEntry {
            handle: message_id,
            account,
            content_id,
            blob_path: self
                .mail_root
                .join(&blob_relpath)
                .to_string_lossy()
                .to_string(),
            local_role,
            read_state,
            starred,
            date: row.get(8)?,
            from: row.get(9)?,
            to: row.get(10)?,
            cc: row.get(11)?,
            bcc: row.get(12)?,
            subject: row.get(13)?,
            rfc_message_id: normalized_message_id.unwrap_or_default(),
            remote,
        })
    }
}

/// Build a stable handle from raw message bytes.
#[must_use] 
pub fn handle_from_bytes(data: &[u8]) -> String {
    let hash = Sha256::digest(data);
    hex::encode(&hash[..HANDLE_LENGTH / 2]).clone()
}

/// Compute SHA-256 fingerprint of raw bytes.
#[must_use] 
pub fn fingerprint(data: &[u8]) -> String {
    let hash = Sha256::digest(data);
    hex::encode(hash)
}

fn canonical_folder(folder: &str) -> &'static str {
    match folder.to_ascii_lowercase().as_str() {
        "archive" | "archives" | "all" => "Archive",
        "trash" | "deleted" => "Trash",
        "sent" => "Sent",
        "draft" | "drafts" => "Drafts",
        "outbox" => "outbox",
        _ => "INBOX",
    }
}

fn catalog_select_sql() -> &'static str {
    "SELECT
        m.message_id,
        m.account,
        m.content_id,
        b.blob_relpath,
        b.byte_size,
        m.local_role,
        m.read_state,
        m.starred,
        md.date,
        md.from_addr,
        md.to_addr,
        md.cc_addr,
        md.bcc_addr,
        md.subject,
        md.normalized_message_id,
        rb.account,
        rb.provider,
        rb.remote_mailbox,
        rb.remote_uid,
        rb.remote_uidvalidity
     FROM messages m
     JOIN blobs b ON b.content_id = m.content_id
     JOIN message_metadata md ON md.content_id = m.content_id
     LEFT JOIN remote_bindings rb ON rb.message_id = m.message_id"
}

impl Catalog {
    fn entry_bytes(&self, entry: &CatalogEntry) -> Vec<u8> {
        if let Ok(data) = fs::read(&entry.blob_path) {
            return data;
        }
        let path = Path::new(&entry.blob_path);
        if path.is_relative()
            && let Ok(data) = fs::read(self.mail_root.join(path))
        {
            return data;
        }
        synthesized_entry_bytes(entry)
    }
}

fn synthesized_entry_bytes(entry: &CatalogEntry) -> Vec<u8> {
    let mut headers = Vec::new();
    if !entry.date.is_empty() {
        headers.push(format!("Date: {}", entry.date));
    }
    if !entry.from.is_empty() {
        headers.push(format!("From: {}", entry.from));
    }
    if !entry.to.is_empty() {
        headers.push(format!("To: {}", entry.to));
    }
    if !entry.cc.is_empty() {
        headers.push(format!("Cc: {}", entry.cc));
    }
    if !entry.bcc.is_empty() {
        headers.push(format!("Bcc: {}", entry.bcc));
    }
    if !entry.subject.is_empty() {
        headers.push(format!("Subject: {}", entry.subject));
    }
    if !entry.rfc_message_id.is_empty() {
        headers.push(format!("Message-ID: <{}>", entry.rfc_message_id));
    }
    headers.push(String::new());
    headers.join("\r\n").into_bytes()
}

fn local_role_from_folder(folder: &str) -> String {
    match canonical_folder(folder) {
        "INBOX" => "inbox".into(),
        "Archive" => "archive".into(),
        "Trash" => "trash".into(),
        "Sent" => "sent".into(),
        "Drafts" => "drafts".into(),
        other => other.to_ascii_lowercase(),
    }
}

fn remote_binding_input(remote: &RemoteIdentity) -> RemoteBindingInput {
    RemoteBindingInput {
        account: remote.account.clone(),
        provider: remote.provider.clone(),
        remote_mailbox: remote.remote_mailbox.clone(),
        remote_uid: remote.uid,
        remote_uidvalidity: remote.uidvalidity,
    }
}

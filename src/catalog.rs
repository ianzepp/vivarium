use std::collections::HashMap;
use std::fs;
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use hex;
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::error::VivariumError;
use crate::message::{MessageEntry, message_id_from_bytes};
use crate::storage::{MessageIngestRequest, RemoteBindingInput, Storage};
use crate::store::secure_create_dir_all;

mod local;
mod remote;
#[allow(dead_code)]
mod sqlite;
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

/// Legacy catalog file name imported on first SQLite open.
const LEGACY_CATALOG_FILENAME: &str = "catalog.json";

/// Stable handle prefix length (hex chars of SHA-256).
const HANDLE_LENGTH: usize = 16;

/// A message row in the catalog.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CatalogEntry {
    pub handle: String,
    pub raw_path: String,
    pub fingerprint: String,
    pub account: String,
    pub folder: String,
    pub maildir_subdir: String,
    pub date: String,
    pub from: String,
    pub to: String,
    pub cc: String,
    pub bcc: String,
    pub subject: String,
    pub rfc_message_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub remote: Option<RemoteIdentity>,
    pub is_duplicate: bool,
}

#[derive(Debug, Default)]
pub struct CatalogUpdateResult {
    pub scanned: usize,
    pub cataloged: usize,
    pub skipped: usize,
    pub duplicates: usize,
    pub entries: Vec<CatalogEntry>,
}

/// The mail catalog backed by SQLite.
pub struct Catalog {
    mail_root: PathBuf,
    conn: Connection,
}

impl Catalog {
    /// Open or create the catalog at the given mail root.
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

        ensure_catalog_compat_schema(&conn)?;

        let mut catalog = Self {
            mail_root: mail_root.to_path_buf(),
            conn,
        };
        catalog.import_legacy_json_if_needed(&catalog_dir.join(LEGACY_CATALOG_FILENAME))?;

        Ok(catalog)
    }

    /// SQLite autocommit persists writes; this keeps the old internal API shape.
    fn flush(&self) -> Result<(), VivariumError> {
        Ok(())
    }

    /// Insert or update a single message in the catalog.
    pub fn upsert(&mut self, entry: &CatalogEntry) -> Result<(), VivariumError> {
        let data = entry_bytes(entry)?;
        let request = MessageIngestRequest {
            account: entry.account.clone(),
            local_role: local_role_from_folder(&entry.folder),
            read_state: entry.maildir_subdir == "cur",
            starred: raw_path_has_maildir_flag(&entry.raw_path, 'F'),
            message_id_hint: Some(entry.handle.clone()),
            seed_hint: entry.handle.clone(),
            remote: entry.remote.as_ref().map(remote_binding_input),
        };
        Storage::open(&self.mail_root)?.ingest_message(&request, &data)?;
        self.upsert_catalog_compat(entry)?;
        self.flush()?;
        Ok(())
    }

    /// List all catalog entries for an account.
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

    /// Look up a message handle by raw file path.
    pub fn handle_for_path(&self, path: &str) -> Result<Option<String>, VivariumError> {
        if let Some(handle) = self
            .conn
            .query_row(
                "SELECT message_id FROM catalog_compat WHERE raw_path = ?1 LIMIT 1",
                params![path],
                |row| row.get(0),
            )
            .optional()
            .map_err(|e| VivariumError::Other(format!("failed to read catalog handle: {e}")))?
        {
            return Ok(Some(handle));
        }

        let relpath = Path::new(path)
            .strip_prefix(&self.mail_root)
            .ok()
            .map(|p| p.to_string_lossy().to_string());
        let Some(relpath) = relpath else {
            return Ok(None);
        };
        self.conn
            .query_row(
                "SELECT m.message_id
                 FROM messages m
                 JOIN blobs b ON b.content_id = m.content_id
                 WHERE b.blob_relpath = ?1 AND m.deleted_at IS NULL
                 LIMIT 1",
                params![relpath],
                |row| row.get(0),
            )
            .optional()
            .map_err(|e| {
                VivariumError::Other(format!("failed to read storage-backed catalog handle: {e}"))
            })
    }

    /// Remove all entries for an account from the catalog.
    pub fn remove_account(&mut self, account: &str) -> Result<(), VivariumError> {
        self.conn
            .execute(
                "DELETE FROM catalog_compat
                 WHERE message_id IN (SELECT message_id FROM messages WHERE account = ?1)",
                params![account],
            )
            .map_err(|e| {
                VivariumError::Other(format!("failed to remove catalog compatibility rows: {e}"))
            })?;
        self.conn
            .execute("DELETE FROM messages WHERE account = ?1", params![account])
            .map_err(|e| VivariumError::Other(format!("failed to remove catalog account: {e}")))?;
        self.flush()?;
        Ok(())
    }

    /// Count entries for an account.
    pub fn count_messages(&self, account: &str) -> Result<usize, VivariumError> {
        self.conn
            .query_row(
                "SELECT COUNT(*) FROM messages WHERE account = ?1 AND deleted_at IS NULL",
                params![account],
                |row| row.get(0),
            )
            .map_err(|e| VivariumError::Other(format!("failed to count catalog rows: {e}")))
    }

    fn import_legacy_json_if_needed(&mut self, legacy_path: &Path) -> Result<(), VivariumError> {
        if !legacy_path.exists() {
            return Ok(());
        }
        let count: usize = self
            .conn
            .query_row("SELECT COUNT(*) FROM messages", [], |row| row.get(0))
            .map_err(|e| VivariumError::Other(format!("failed to count stored messages: {e}")))?;
        if count > 0 {
            return Ok(());
        }
        let data = fs::read_to_string(legacy_path)?;
        let entries = serde_json::from_str::<Vec<CatalogEntry>>(&data).map_err(|e| {
            VivariumError::Other(format!("failed to parse legacy catalog JSON: {e}"))
        })?;
        for entry in entries {
            self.upsert(&entry)?;
        }
        Ok(())
    }

    fn upsert_catalog_compat(&self, entry: &CatalogEntry) -> Result<(), VivariumError> {
        self.conn
            .execute(
                "INSERT INTO catalog_compat (
                   message_id, raw_path, folder, maildir_subdir, fingerprint, is_duplicate
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6)
                 ON CONFLICT(message_id) DO UPDATE SET
                   raw_path = excluded.raw_path,
                   folder = excluded.folder,
                   maildir_subdir = excluded.maildir_subdir,
                   fingerprint = excluded.fingerprint,
                   is_duplicate = excluded.is_duplicate",
                params![
                    entry.handle,
                    entry.raw_path,
                    entry.folder,
                    entry.maildir_subdir,
                    entry.fingerprint,
                    if entry.is_duplicate { 1 } else { 0 },
                ],
            )
            .map_err(|e| {
                VivariumError::Other(format!("failed to upsert catalog compatibility row: {e}"))
            })?;
        Ok(())
    }

    fn catalog_entry_from_row(&self, row: &rusqlite::Row<'_>) -> rusqlite::Result<CatalogEntry> {
        let message_id: String = row.get(0)?;
        let account: String = row.get(1)?;
        let content_id: String = row.get(2)?;
        let blob_relpath: String = row.get(3)?;
        let local_role: String = row.get(5)?;
        let read_state = row.get::<_, i64>(6)? != 0;
        let normalized_message_id: Option<String> = row.get(14)?;
        let remote_account: Option<String> = row.get(15)?;
        let compat_raw_path: Option<String> = row.get(20)?;
        let compat_folder: Option<String> = row.get(21)?;
        let compat_maildir_subdir: Option<String> = row.get(22)?;
        let compat_fingerprint: Option<String> = row.get(23)?;
        let compat_is_duplicate = row.get::<_, Option<i64>>(24)?.unwrap_or(0) != 0;

        let raw_path = compat_raw_path.unwrap_or_else(|| {
            self.mail_root
                .join(&blob_relpath)
                .to_string_lossy()
                .to_string()
        });
        let folder = compat_folder.unwrap_or_else(|| folder_name_from_role(&local_role));
        let maildir_subdir = compat_maildir_subdir.unwrap_or_else(|| {
            if read_state {
                "cur".into()
            } else {
                "new".into()
            }
        });
        let fingerprint = compat_fingerprint.unwrap_or(content_id.clone());
        let remote = remote_account.map(|remote_account| RemoteIdentity {
            account: remote_account,
            provider: row.get(16).unwrap_or_default(),
            remote_mailbox: row.get(17).unwrap_or_default(),
            local_folder: local_role.clone(),
            uid: row.get(18).unwrap_or_default(),
            uidvalidity: row.get(19).unwrap_or_default(),
            rfc_message_id: normalized_message_id.clone().unwrap_or_default(),
            size: row.get::<_, i64>(4).unwrap_or_default() as u64,
            content_fingerprint: fingerprint.clone(),
        });

        Ok(CatalogEntry {
            handle: message_id,
            raw_path,
            fingerprint,
            account,
            folder,
            maildir_subdir,
            date: row.get(8)?,
            from: row.get(9)?,
            to: row.get(10)?,
            cc: row.get(11)?,
            bcc: row.get(12)?,
            subject: row.get(13)?,
            rfc_message_id: normalized_message_id.unwrap_or_default(),
            remote,
            is_duplicate: compat_is_duplicate,
        })
    }
}

/// Incrementally catalog local Maildir files that do not already have entries.
pub fn update_maildir(
    mail_root: &Path,
    account: &str,
    store: &crate::store::MailStore,
) -> Result<CatalogUpdateResult, VivariumError> {
    let mut catalog = Catalog::open(mail_root)?;
    let existing = catalog.list_messages(account)?;
    let mut existing_paths: HashMap<String, String> = existing
        .iter()
        .map(|entry| (entry.raw_path.clone(), entry.handle.clone()))
        .collect();
    let mut existing_handles: HashMap<String, CatalogEntry> = existing
        .into_iter()
        .map(|entry| (entry.handle.clone(), entry))
        .collect();
    let mut result = CatalogUpdateResult::default();

    for (folder, subdir, path) in message_paths(store)? {
        result.scanned += 1;
        let raw_path = path.to_string_lossy().to_string();
        if existing_paths.contains_key(&raw_path) {
            result.skipped += 1;
            continue;
        }

        let entry = catalog_entry_for_file(&path, account, &folder, &subdir, &existing_handles);
        if entry.is_duplicate {
            result.duplicates += 1;
        }
        catalog.upsert(&entry)?;
        existing_paths.insert(entry.raw_path.clone(), entry.handle.clone());
        existing_handles.insert(entry.handle.clone(), entry.clone());
        result.cataloged += 1;
        result.entries.push(entry);
    }

    Ok(result)
}

/// Build a stable handle from raw message bytes.
pub fn handle_from_bytes(data: &[u8]) -> String {
    let hash = Sha256::digest(data);
    hex::encode(&hash[..HANDLE_LENGTH / 2]).to_string()
}

/// Compute SHA-256 fingerprint of raw bytes.
pub fn fingerprint(data: &[u8]) -> String {
    let hash = Sha256::digest(data);
    hex::encode(hash)
}

/// Scan raw Maildir files for an account and build catalog entries.
pub fn scan_maildir(
    mail_root: &Path,
    account: &str,
    store: &crate::store::MailStore,
) -> Result<(Vec<CatalogEntry>, usize, usize, usize), VivariumError> {
    let catalog = Catalog::open(mail_root)?;
    let folders = ["INBOX", "Archive", "Sent", "Drafts"];
    let mut all_entries = Vec::new();

    // Load existing catalog entries for dedup
    let existing: HashMap<String, CatalogEntry> = catalog
        .list_messages(account)?
        .into_iter()
        .map(|e| (e.handle.clone(), e))
        .collect();

    for folder in folders {
        let entries_for_folder = scan_folder_with(&existing, folder, account, store)?;
        all_entries.extend(entries_for_folder);
    }

    let new_count = all_entries.iter().filter(|e| !e.is_duplicate).count();
    let dup_count = all_entries.iter().filter(|e| e.is_duplicate).count();
    let total = all_entries.len();
    Ok((all_entries, new_count, dup_count, total))
}

fn scan_folder_with(
    existing: &HashMap<String, CatalogEntry>,
    folder: &str,
    account: &str,
    store: &crate::store::MailStore,
) -> Result<Vec<CatalogEntry>, VivariumError> {
    let mut entries = Vec::new();
    let canonical = canonical_folder(folder);

    for subdir in ["new", "cur"] {
        let dir = store.folder_path(canonical).join(subdir);
        if !dir.exists() {
            continue;
        }
        let folder_entries = scan_subdir(&dir, account, canonical, subdir, existing)?;
        entries.extend(folder_entries);
    }

    Ok(entries)
}

fn scan_subdir(
    dir: &std::path::Path,
    account: &str,
    folder: &str,
    subdir: &str,
    existing: &HashMap<String, CatalogEntry>,
) -> Result<Vec<CatalogEntry>, VivariumError> {
    let mut entries = Vec::new();

    if let Ok(read_dir) = fs::read_dir(dir) {
        for entry_result in read_dir {
            let entry = entry_result.ok();
            let path = entry.as_ref().map(|e| e.path());
            let is_file = entry
                .as_ref()
                .map(|e| e.file_type().ok().map(|ft| ft.is_file()).unwrap_or(false))
                .unwrap_or(false);
            if !is_file || path.is_none() {
                continue;
            }
            let path_val = path.unwrap();
            let stem = path_val
                .file_stem()
                .map(|s| s.to_string_lossy())
                .unwrap_or_default();
            if !stem.ends_with(".eml") {
                continue;
            }
            let entry = catalog_entry_for_file(&path_val, account, folder, subdir, existing);
            entries.push(entry);
        }
    }

    Ok(entries)
}

fn message_paths(
    store: &crate::store::MailStore,
) -> Result<Vec<(String, String, PathBuf)>, VivariumError> {
    let mut paths = Vec::new();
    for folder in ["INBOX", "Archive", "Trash", "Sent", "Drafts"] {
        for subdir in ["new", "cur"] {
            let dir = store.folder_path(folder).join(subdir);
            if !dir.exists() {
                continue;
            }
            for entry in fs::read_dir(dir)? {
                let entry = entry?;
                let path = entry.path();
                if is_message_file(&path) {
                    paths.push((folder.to_string(), subdir.to_string(), path));
                }
            }
        }
    }
    Ok(paths)
}

fn is_message_file(path: &Path) -> bool {
    path.file_name()
        .and_then(|n| n.to_str())
        .map(|name| {
            name.split_once(":2,")
                .map_or(name, |(id, _)| id)
                .ends_with(".eml")
        })
        .unwrap_or(false)
}

fn catalog_entry_for_file(
    path_val: &std::path::Path,
    account: &str,
    folder: &str,
    subdir: &str,
    existing: &HashMap<String, CatalogEntry>,
) -> CatalogEntry {
    let data = fs::read(path_val).ok().unwrap_or_default();
    let handle = handle_from_bytes(&data);
    let fingerprint_val = fingerprint(&data);
    let is_dup = existing.contains_key(&handle);

    let msg_entry = MessageEntry::from_path(path_val).ok();
    let date = msg_entry
        .as_ref()
        .map(|m| m.date.format("%Y-%m-%d %H:%M").to_string())
        .unwrap_or_default();
    let from = msg_entry
        .as_ref()
        .map(|m| m.from.clone())
        .unwrap_or_default();
    let subject = msg_entry
        .as_ref()
        .map(|m| m.subject.clone())
        .unwrap_or_default();
    let rfc_message_id = message_id_from_bytes(&data).unwrap_or_default();

    CatalogEntry {
        handle,
        raw_path: path_val.to_string_lossy().to_string(),
        fingerprint: fingerprint_val,
        account: account.to_string(),
        folder: folder.to_string(),
        maildir_subdir: subdir.to_string(),
        date,
        from,
        to: String::new(),
        cc: String::new(),
        bcc: String::new(),
        subject,
        rfc_message_id,
        remote: None,
        is_duplicate: is_dup,
    }
}

fn canonical_folder(folder: &str) -> &'static str {
    match folder.to_ascii_lowercase().as_str() {
        "inbox" | "new" => "INBOX",
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
        rb.remote_uidvalidity,
        cc.raw_path,
        cc.folder,
        cc.maildir_subdir,
        cc.fingerprint,
        cc.is_duplicate
     FROM messages m
     JOIN blobs b ON b.content_id = m.content_id
     JOIN message_metadata md ON md.content_id = m.content_id
     LEFT JOIN remote_bindings rb ON rb.message_id = m.message_id
     LEFT JOIN catalog_compat cc ON cc.message_id = m.message_id"
}

fn ensure_catalog_compat_schema(conn: &Connection) -> Result<(), VivariumError> {
    conn.execute_batch(
        "BEGIN;
         CREATE TABLE IF NOT EXISTS catalog_compat (
           message_id TEXT PRIMARY KEY REFERENCES messages(message_id) ON DELETE CASCADE,
           raw_path TEXT,
           folder TEXT,
           maildir_subdir TEXT,
           fingerprint TEXT,
           is_duplicate INTEGER NOT NULL DEFAULT 0
         );
         COMMIT;",
    )
    .map_err(|e| {
        VivariumError::Other(format!(
            "failed to initialize catalog compatibility schema: {e}"
        ))
    })
}

fn entry_bytes(entry: &CatalogEntry) -> Result<Vec<u8>, VivariumError> {
    match fs::read(&entry.raw_path) {
        Ok(data) => Ok(data),
        Err(_) => Ok(synthesized_entry_bytes(entry)),
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

fn folder_name_from_role(local_role: &str) -> String {
    match local_role {
        "inbox" => "INBOX".into(),
        "archive" => "Archive".into(),
        "trash" => "Trash".into(),
        "sent" => "Sent".into(),
        "drafts" | "draft" => "Drafts".into(),
        other => other.to_string(),
    }
}

fn raw_path_has_maildir_flag(path: &str, flag: char) -> bool {
    path.rsplit_once(":2,")
        .map(|(_, flags)| flags.contains(flag))
        .unwrap_or(false)
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

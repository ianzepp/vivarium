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
use crate::store::secure_create_dir_all;

mod local;
mod remote;
mod sqlite;
#[cfg(test)]
mod tests;

pub use remote::{
    RemoteIdentity, RemoteIdentityAttachResult, RemoteIdentityCandidate, RemoteReferenceStatus,
    attach_remote_identities,
};
use sqlite::{catalog_entry_from_row, ensure_schema, import_legacy_json_if_needed, upsert_entry};

/// Catalog directory inside the mail root.
const CATALOG_DIR: &str = ".vivarium";

/// Catalog SQLite database filename.
const CATALOG_DB_FILENAME: &str = "catalog.sqlite";

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
    conn: Connection,
}

impl Catalog {
    /// Open or create the catalog at the given mail root.
    pub fn open(mail_root: &Path) -> Result<Self, VivariumError> {
        let catalog_dir = mail_root.join(CATALOG_DIR);
        secure_create_dir_all(&catalog_dir)
            .map_err(|e| VivariumError::Other(format!("failed to create catalog dir: {e}")))?;
        let catalog_path = catalog_dir.join(CATALOG_DB_FILENAME);
        let conn = Connection::open(&catalog_path)
            .map_err(|e| VivariumError::Other(format!("failed to open catalog database: {e}")))?;
        #[cfg(unix)]
        fs::set_permissions(&catalog_path, fs::Permissions::from_mode(0o600))?;

        ensure_schema(&conn)?;
        import_legacy_json_if_needed(&conn, &catalog_dir.join(LEGACY_CATALOG_FILENAME))?;

        Ok(Self { conn })
    }

    /// SQLite autocommit persists writes; this keeps the old internal API shape.
    fn flush(&self) -> Result<(), VivariumError> {
        Ok(())
    }

    /// Insert or update a single message in the catalog.
    pub fn upsert(&mut self, entry: &CatalogEntry) -> Result<(), VivariumError> {
        upsert_entry(&self.conn, entry)?;
        self.flush()?;
        Ok(())
    }

    /// List all catalog entries for an account.
    pub fn list_messages(&self, account: &str) -> Result<Vec<CatalogEntry>, VivariumError> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT handle, raw_path, fingerprint, account, folder, maildir_subdir,
                        date, from_addr, to_addr, cc_addr, bcc_addr, subject,
                        rfc_message_id, remote_json, is_duplicate
                 FROM catalog_entries
                 WHERE account = ?1
                 ORDER BY date DESC, handle",
            )
            .map_err(|e| VivariumError::Other(format!("failed to prepare catalog listing: {e}")))?;
        let rows = stmt
            .query_map(params![account], catalog_entry_from_row)
            .map_err(|e| VivariumError::Other(format!("failed to list catalog rows: {e}")))?;
        rows.map(|row| {
            row.map_err(|e| VivariumError::Other(format!("failed to read catalog row: {e}")))
        })
        .collect()
    }

    /// Look up a message handle by raw file path.
    pub fn handle_for_path(&self, path: &str) -> Result<Option<String>, VivariumError> {
        self.conn
            .query_row(
                "SELECT handle FROM catalog_entries WHERE raw_path = ?1 LIMIT 1",
                params![path],
                |row| row.get(0),
            )
            .optional()
            .map_err(|e| VivariumError::Other(format!("failed to read catalog handle: {e}")))
    }

    /// Remove all entries for an account from the catalog.
    pub fn remove_account(&mut self, account: &str) -> Result<(), VivariumError> {
        self.conn
            .execute(
                "DELETE FROM catalog_entries WHERE account = ?1",
                params![account],
            )
            .map_err(|e| VivariumError::Other(format!("failed to remove catalog account: {e}")))?;
        self.flush()?;
        Ok(())
    }

    /// Count entries for an account.
    pub fn count_messages(&self, account: &str) -> Result<usize, VivariumError> {
        self.conn
            .query_row(
                "SELECT COUNT(*) FROM catalog_entries WHERE account = ?1",
                params![account],
                |row| row.get(0),
            )
            .map_err(|e| VivariumError::Other(format!("failed to count catalog rows: {e}")))
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

        let entry =
            catalog_entry_for_file(&path, account, &folder, &subdir, existing_handles.clone());
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
        let entries_for_folder = scan_folder_with(existing.clone(), folder, account, store)?;
        all_entries.extend(entries_for_folder);
    }

    let new_count = all_entries.iter().filter(|e| !e.is_duplicate).count();
    let dup_count = all_entries.iter().filter(|e| e.is_duplicate).count();
    let total = all_entries.len();
    Ok((all_entries, new_count, dup_count, total))
}

fn scan_folder_with(
    existing: HashMap<String, CatalogEntry>,
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
        let folder_entries = scan_subdir(&dir, account, canonical, subdir, existing.clone())?;
        entries.extend(folder_entries);
    }

    Ok(entries)
}

fn scan_subdir(
    dir: &std::path::Path,
    account: &str,
    folder: &str,
    subdir: &str,
    existing: HashMap<String, CatalogEntry>,
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
            let entry =
                catalog_entry_for_file(&path_val, account, folder, subdir, existing.clone());
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
    existing: HashMap<String, CatalogEntry>,
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

use std::collections::HashMap;
use std::fs;
use std::path::Path;

use hex;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::error::VivariumError;
use crate::message::MessageEntry;
use crate::store::{secure_create_dir_all, secure_write};

/// Catalog directory inside the mail root.
const CATALOG_DIR: &str = ".vivarium";

/// Catalog file name.
const CATALOG_FILENAME: &str = "catalog.json";

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
    pub is_duplicate: bool,
}

/// The mail catalog backed by a JSON index.
pub struct Catalog {
    root: String,
    entries: HashMap<String, CatalogEntry>,
}

impl Catalog {
    /// Open or create the catalog at the given mail root.
    pub fn open(mail_root: &Path) -> Result<Self, VivariumError> {
        let catalog_dir = mail_root.join(CATALOG_DIR);
        secure_create_dir_all(&catalog_dir).map_err(|e| {
            VivariumError::Other(format!("failed to create catalog dir: {e}"))
        })?;
        let catalog_path = catalog_dir.join(CATALOG_FILENAME);

        let mut entries = HashMap::new();
        if catalog_path.exists() {
            if let Ok(data) = fs::read_to_string(&catalog_path) {
                if let Ok(loaded) = serde_json::from_str::<Vec<CatalogEntry>>(&data) {
                    for e in loaded {
                        entries.insert(e.handle.clone(), e);
                    }
                } else {
                    tracing::warn!("failed to load catalog");
                }
            }
        }

        Ok(Self {
            root: mail_root.to_string_lossy().to_string(),
            entries,
        })
    }

    /// Load entries from disk (for rebuilds).
    /// Persist entries to disk.
    fn flush(&self) -> Result<(), VivariumError> {
        let catalog_path = format!("{}/{}/{}", self.root, CATALOG_DIR, CATALOG_FILENAME);
        let entries: Vec<CatalogEntry> = self.entries.values().cloned().collect();
        let json = serde_json::to_string_pretty(&entries).map_err(|e| {
            VivariumError::Other(format!("catalog serialization failed: {e}"))
        })?;
        secure_write(Path::new(&catalog_path), json.as_bytes())?;
        Ok(())
    }

    /// Insert or update a single message in the catalog.
    pub fn upsert(&mut self, entry: &CatalogEntry) -> Result<(), VivariumError> {
        self.entries.insert(entry.handle.clone(), entry.clone());
        self.flush()?;
        Ok(())
    }

    /// List all catalog entries for an account.
    pub fn list_messages(&self, account: &str) -> Result<Vec<CatalogEntry>, VivariumError> {
        let mut entries: Vec<CatalogEntry> = self.entries.values()
            .filter(|e| e.account == account)
            .cloned()
            .collect();
        entries.sort_by(|a, b| b.date.cmp(&a.date));
        Ok(entries)
    }

    /// Look up a message handle by raw file path.
    pub fn handle_for_path(&self, path: &str) -> Result<Option<String>, VivariumError> {
        let entry = self.entries.values().find(|e| e.raw_path == path);
        Ok(entry.map(|e| e.handle.clone()))
    }

    /// Remove all entries for an account from the catalog.
    pub fn remove_account(&mut self, account: &str) -> Result<(), VivariumError> {
        self.entries.retain(|_, e| e.account != account);
        self.flush()?;
        Ok(())
    }

    /// Count entries for an account.
    pub fn count_messages(&self, account: &str) -> Result<usize, VivariumError> {
        Ok(self.entries.values().filter(|e| e.account == account).count())
    }
}

/// Build a stable handle from raw message bytes.
pub fn handle_from_bytes(data: &[u8]) -> String {
    let hash = Sha256::digest(data);
    hex::encode(&hash[..HANDLE_LENGTH / 2]).to_string()
}

/// Compute SHA-256 fingerprint of raw bytes.
pub fn fingerprint(data: &[u8]) -> String {
    let hash = Sha256::digest(data);
    hex::encode(&hash)
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
    let existing: HashMap<String, CatalogEntry> = catalog.list_messages(account)?
        .into_iter().map(|e| (e.handle.clone(), e)).collect();

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
            let is_file = entry.as_ref().map(|e| {
                e.file_type().ok().map(|ft| ft.is_file()).unwrap_or(false)
            }).unwrap_or(false);
            if !is_file || path.is_none() {
                continue;
            }
            let path_val = path.unwrap();
            let stem = path_val.file_stem().map(|s| s.to_string_lossy()).unwrap_or_default();
            if !stem.ends_with(".eml") {
                continue;
            }
            let entry = catalog_entry_for_file(&path_val, account, folder, subdir, existing.clone());
            entries.push(entry);
        }
    }

    Ok(entries)
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
    let date = msg_entry.as_ref().map(|m| m.date.format("%Y-%m-%d %H:%M").to_string()).unwrap_or_default();
    let from = msg_entry.as_ref().map(|m| m.from.clone()).unwrap_or_default();
    let subject = msg_entry.as_ref().map(|m| m.subject.clone()).unwrap_or_default();

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
        rfc_message_id: String::new(),
        is_duplicate: is_dup,
    }
}

fn canonical_folder(folder: &str) -> &'static str {
    match folder.to_ascii_lowercase().as_str() {
        "inbox" | "new" => "INBOX",
        "archive" | "archives" | "all" => "Archive",
        "sent" => "Sent",
        "draft" | "drafts" => "Drafts",
        "outbox" => "outbox",
        _ => "INBOX",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    #[cfg(unix)]
    use std::os::unix::fs::PermissionsExt;

    #[test]
    fn handle_is_stable_for_same_content() {
        let data = b"Subject: test\r\nFrom: a@b\r\nTo: c@d\r\n\r\nhello";
        let h1 = handle_from_bytes(data);
        let h2 = handle_from_bytes(data);
        assert_eq!(h1, h2);
    }

    #[test]
    fn handle_differs_for_different_content() {
        let data1 = b"Subject: test1\r\n\r\na";
        let data2 = b"Subject: test2\r\n\r\na";
        assert_ne!(handle_from_bytes(data1), handle_from_bytes(data2));
    }

    #[test]
    fn catalog_opens_and_closes() {
        let tmp = tempfile::tempdir().unwrap();
        let store = crate::store::MailStore::new(tmp.path());
        let catalog = Catalog::open(store.root()).unwrap();
        assert_eq!(catalog.count_messages("test").unwrap(), 0);
    }

    #[cfg(unix)]
    #[test]
    fn catalog_uses_private_permissions() {
        let tmp = tempfile::tempdir().unwrap();
        let store = crate::store::MailStore::new(tmp.path());
        let mut catalog = Catalog::open(store.root()).unwrap();
        let entry = CatalogEntry {
            handle: "abc123".into(),
            raw_path: "/test.msg".into(),
            fingerprint: "f1".into(),
            account: "acct".into(),
            folder: "INBOX".into(),
            maildir_subdir: "new".into(),
            date: "2025-01-01 00:00".into(),
            from: "a@b".into(),
            to: "c@d".into(),
            cc: String::new(),
            bcc: String::new(),
            subject: "hi".into(),
            rfc_message_id: String::new(),
            is_duplicate: false,
        };

        catalog.upsert(&entry).unwrap();

        assert_eq!(mode(&store.root().join(".vivarium")), 0o700);
        assert_eq!(mode(&store.root().join(".vivarium/catalog.json")), 0o600);
    }

    #[test]
    fn catalog_upsert_and_list() {
        let tmp = tempfile::tempdir().unwrap();
        let store = crate::store::MailStore::new(tmp.path());
        let mut catalog = Catalog::open(store.root()).unwrap();

        let entry = CatalogEntry {
            handle: "abc123".into(),
            raw_path: "/test.msg".into(),
            fingerprint: "f1".into(),
            account: "acct".into(),
            folder: "INBOX".into(),
            maildir_subdir: "new".into(),
            date: "2025-01-01 00:00".into(),
            from: "a@b".into(),
            to: "c@d".into(),
            cc: String::new(),
            bcc: String::new(),
            subject: "hi".into(),
            rfc_message_id: String::new(),
            is_duplicate: false,
        };

        catalog.upsert(&entry).unwrap();
        assert_eq!(catalog.count_messages("acct").unwrap(), 1);

        let entries = catalog.list_messages("acct").unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].handle, "abc123");
        assert!(!entries[0].is_duplicate);
    }

    #[test]
    fn catalog_rebuild_stable_handles() {
        let tmp = tempfile::tempdir().unwrap();
        let mail_root = tmp.path().join("mail");
        fs::create_dir_all(&mail_root).unwrap();

        // Create a test .eml file
        let msg_data = b"Subject: stable\r\nFrom: a@b\r\nTo: c@d\r\nMessage-ID: <test@example.com>\r\n\r\nbody";
        fs::write(&mail_root.join("inbox-1.eml"), msg_data).unwrap();

        // Handle from first call
        let handle1 = handle_from_bytes(msg_data);
        let fp1 = fingerprint(msg_data);

        // Write to catalog
        let catalog = Catalog::open(&mail_root).unwrap();
        let entry = CatalogEntry {
            handle: handle1.clone(),
            raw_path: "inbox-1.eml".into(),
            fingerprint: fp1.clone(),
            account: "test".into(), folder: "INBOX".into(), maildir_subdir: "new".into(),
            date: "2025".into(), from: "a@b".into(), to: "c@d".into(),
            cc: String::new(), bcc: String::new(), subject: "stable".into(),
            rfc_message_id: "test@example.com".into(), is_duplicate: false,
        };
        let mut catalog = Catalog::open(&mail_root).unwrap();
        catalog.upsert(&entry).unwrap();

        // Re-open and verify handle stability
        let catalog2 = Catalog::open(&mail_root).unwrap();
        let handles = catalog2.list_messages("test").unwrap();
        assert_eq!(handles.len(), 1);
        assert_eq!(handles[0].handle, handle1);
        assert_eq!(handles[0].fingerprint, fp1);
    }

    #[test]
    fn catalog_duplicate_same_handle_replaces() {
        let tmp = tempfile::tempdir().unwrap();
        let mail_root = tmp.path().join("mail");
        fs::create_dir_all(&mail_root).unwrap();

        let msg_data = b"Subject: dup\r\nFrom: a@b\r\nTo: c@d\r\n\r\ndup content";
        let handle = handle_from_bytes(msg_data);

        // Upsert entry to catalog
        let entry = CatalogEntry {
            handle: handle.clone(), raw_path: "INBOX/inbox.eml".into(),
            fingerprint: fingerprint(msg_data),
            account: "test".into(), folder: "INBOX".into(), maildir_subdir: "new".into(),
            date: "2025".into(), from: "a@b".into(), to: "c@d".into(),
            cc: String::new(), bcc: String::new(), subject: "dup".into(),
            rfc_message_id: String::new(), is_duplicate: false,
        };
        let mut catalog = Catalog::open(&mail_root).unwrap();
        catalog.upsert(&entry).unwrap();

        // Second entry with same handle replaces the first
        let dup_entry = CatalogEntry {
            handle: handle.clone(), raw_path: "Archive/cur/dup.eml".into(),
            fingerprint: fingerprint(msg_data),
            account: "test".into(), folder: "Archive".into(), maildir_subdir: "cur".into(),
            date: "2025".into(), from: "a@b".into(), to: "c@d".into(),
            cc: String::new(), bcc: String::new(), subject: "dup".into(),
            rfc_message_id: String::new(), is_duplicate: false,
        };
        catalog.upsert(&dup_entry).unwrap();

        // Same handle means same message: only one entry in catalog
        let entries = catalog.list_messages("test").unwrap();
        assert_eq!(entries.len(), 1);
        // The raw_path should be updated to the last upsert
        assert_eq!(entries[0].raw_path, "Archive/cur/dup.eml");
    }

    #[cfg(unix)]
    fn mode(path: &std::path::Path) -> u32 {
        fs::metadata(path).unwrap().permissions().mode() & 0o777
    }
}

use std::cmp::Reverse;
use std::collections::HashMap;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use crate::error::VivariumError;
use crate::message::{MessageEntry, message_id_from_bytes};

mod path;
mod secure;
#[cfg(test)]
mod tests;

pub use path::message_id_from_path;
use path::{canonical_folder, display_message_id, is_message_file, maildir_filename, stable_hash};
pub(crate) use secure::{secure_create_dir_all, secure_write};
use secure::{secure_create_file, secure_file};

/// Local Maildir folders.
const FOLDERS: &[&str] = &["INBOX", "Archive", "Sent", "Drafts", "outbox"];
const MAILDIR_DIRS: &[&str] = &["tmp", "new", "cur"];

/// File-based mail store for a single account.
///
/// Layout:
/// ```text
/// {root}/
/// ├── INBOX/
/// │   ├── tmp/
/// │   ├── new/
/// │   └── cur/
/// ├── Archive/
/// ├── Sent/
/// ├── Drafts/
/// └── outbox/
/// ```
///
/// Each message is stored as a Maildir entry. Vivarium keeps `.eml` in
/// generated names so the files remain friendly to non-mail tools.
#[derive(Clone)]
pub struct MailStore {
    root: PathBuf,
}

impl MailStore {
    pub fn new(root: &Path) -> Self {
        Self {
            root: root.to_path_buf(),
        }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Create all local Maildir folders if they don't exist.
    pub fn ensure_folders(&self) -> Result<(), VivariumError> {
        for folder in FOLDERS {
            self.ensure_folder(folder)?;
            if *folder == "outbox" {
                let failed = self.folder_path(folder).join("failed");
                if !failed.exists() {
                    fs::create_dir_all(&failed)?;
                    tracing::debug!(path = %failed.display(), "created outbox failed dir");
                }
            }
        }
        Ok(())
    }

    /// Path to a specific folder.
    pub fn folder_path(&self, folder: &str) -> PathBuf {
        self.root.join(canonical_folder(folder).unwrap_or(folder))
    }

    /// List message entries in a folder.
    pub fn list_messages(&self, folder: &str) -> Result<Vec<MessageEntry>, VivariumError> {
        let folder = resolve_folder(folder)?;
        let path = self.folder_path(folder);
        if !path.exists() {
            return Ok(vec![]);
        }

        let mut entries = Vec::new();
        for subdir in ["new", "cur"] {
            let dir = path.join(subdir);
            if !dir.exists() {
                continue;
            }
            for entry in fs::read_dir(&dir)? {
                let entry = entry?;
                let file_path = entry.path();
                if is_message_file(&file_path) {
                    entries.push(MessageEntry::from_path(&file_path)?);
                }
            }
        }
        entries.sort_by_key(|entry| Reverse(entry.date));
        Ok(entries)
    }

    /// Read raw message bytes by message ID (looks across all folders).
    pub fn read_message(&self, message_id: &str) -> Result<Vec<u8>, VivariumError> {
        if let Some(path) = self.find_message(message_id, FOLDERS)? {
            return Ok(fs::read(&path)?);
        }
        Err(VivariumError::Message(format!(
            "message not found: {message_id}"
        )))
    }

    /// Store a message in `new/`.
    pub fn store_message(
        &self,
        folder: &str,
        message_id: &str,
        data: &[u8],
    ) -> Result<PathBuf, VivariumError> {
        self.store_message_in(folder, "new", message_id, data)
    }

    /// Store a message in a specific Maildir subdirectory.
    pub fn store_message_in(
        &self,
        folder: &str,
        subdir: &str,
        message_id: &str,
        data: &[u8],
    ) -> Result<PathBuf, VivariumError> {
        let folder = resolve_folder(folder)?;
        if !matches!(subdir, "new" | "cur") {
            return Err(VivariumError::Message(format!(
                "invalid maildir destination: {subdir}"
            )));
        }

        self.ensure_folder(folder)?;
        let folder_path = self.folder_path(folder);
        let filename = maildir_filename(message_id, subdir);
        let tmp_path = folder_path.join("tmp").join(&filename);
        let final_path = folder_path.join(subdir).join(&filename);

        let mut file = secure_create_file(&tmp_path)?;
        file.write_all(data)?;
        file.sync_all()?;
        fs::rename(&tmp_path, &final_path)?;
        secure_file(&final_path)?;
        tracing::debug!(path = %final_path.display(), "stored message");
        Ok(final_path)
    }

    /// Move a message between folders (e.g. inbox -> archive).
    pub fn move_message(
        &self,
        message_id: &str,
        from: &str,
        to: &str,
    ) -> Result<PathBuf, VivariumError> {
        let from_folder = resolve_folder(from)?;
        let to_folder = resolve_folder(to)?;
        self.ensure_folder(to)?;
        let src = self
            .find_message_in_subdirs(message_id, from_folder, MAILDIR_DIRS)?
            .ok_or_else(|| {
                VivariumError::Message(format!("message not found in {from}: {message_id}"))
            })?;
        let subdir = src
            .parent()
            .and_then(|p| p.file_name())
            .and_then(|n| n.to_str())
            .ok_or_else(|| VivariumError::Message("message path has no parent".into()))?;
        if !matches!(subdir, "new" | "cur") {
            return Err(VivariumError::Message(format!(
                "cannot move message from unexpected maildir subdirectory '{subdir}': {}",
                src.display()
            )));
        }
        let filename = src
            .file_name()
            .ok_or_else(|| VivariumError::Message("message path has no filename".into()))?;
        let dst = self.folder_path(to_folder).join(subdir).join(filename);

        fs::rename(&src, &dst)?;
        tracing::debug!(from = %src.display(), to = %dst.display(), "moved message");
        Ok(dst)
    }

    /// Check if a message exists in any folder.
    pub fn contains(&self, message_id: &str) -> bool {
        self.find_message(message_id, FOLDERS)
            .map(|p| p.is_some())
            .unwrap_or(false)
    }

    /// Get the file size of a message in a specific folder, if it exists.
    pub fn file_size(&self, folder: &str, message_id: &str) -> Option<u64> {
        let folder = resolve_folder(folder).ok()?;
        self.find_message(message_id, &[folder])
            .ok()
            .flatten()
            .and_then(|path| fs::metadata(&path).ok().map(|m| m.len()))
    }

    /// Build a map of message_id -> file_size for all message files in a folder.
    pub fn local_sizes(&self, folder: &str) -> Result<HashMap<String, u64>, VivariumError> {
        let folder = resolve_folder(folder)?;
        let path = self.folder_path(folder);
        let mut map = HashMap::new();
        if !path.exists() {
            return Ok(map);
        }

        for subdir in ["new", "cur"] {
            let dir = path.join(subdir);
            if !dir.exists() {
                continue;
            }
            for entry in fs::read_dir(&dir)? {
                let entry = entry?;
                let file_path = entry.path();
                if is_message_file(&file_path)
                    && let Some(id) = message_id_from_path(&file_path)
                {
                    let size = fs::metadata(&file_path).map(|m| m.len()).unwrap_or(0);
                    map.insert(id, size);
                }
            }
        }
        Ok(map)
    }

    /// Build an in-memory map of RFC 5322 Message-ID → (uid, size) for a folder.
    /// Scans every .eml file in new/ and cur/ once.
    pub fn build_rfc_index(
        &self,
        folder: &str,
    ) -> Result<HashMap<String, (u32, u64)>, VivariumError> {
        let folder = resolve_folder(folder)?;
        let path = self.folder_path(folder);
        let mut map = HashMap::new();
        for subdir in ["new", "cur"] {
            let dir = path.join(subdir);
            if !dir.exists() {
                continue;
            }
            for entry in fs::read_dir(&dir)? {
                let entry = entry?;
                let file_path = entry.path();
                if !is_message_file(&file_path) {
                    continue;
                }
                let data = fs::read(&file_path)?;
                if let Some(rfc_id) = message_id_from_bytes(&data) {
                    let size = fs::metadata(&file_path)?.len();
                    let uid = message_id_from_path(&file_path)
                        .and_then(|id| id.rsplit_once('-').and_then(|(_, uid)| uid.parse().ok()))
                        .unwrap_or(0);
                    map.insert(rfc_id, (uid, size));
                }
            }
        }
        Ok(map)
    }

    /// Check if an RFC 5322 Message-ID exists in the index with a matching size.
    pub fn rfc_index_lookup(
        &self,
        index: &HashMap<String, (u32, u64)>,
        rfc_message_id: &str,
        size: u64,
    ) -> bool {
        index
            .get(rfc_message_id)
            .is_some_and(|(_, indexed_size)| *indexed_size == size)
    }

    /// Check if an RFC 5322 Message-ID exists in the index.
    pub fn rfc_index_contains(
        &self,
        index: &HashMap<String, (u32, u64)>,
        rfc_message_id: &str,
    ) -> bool {
        index.contains_key(rfc_message_id)
    }

    pub fn write_message_index(
        &self,
        folder: &str,
        rfc_message_id: &str,
        uid: u32,
        size: u64,
    ) -> Result<(), VivariumError> {
        let path = self.index_path(folder, rfc_message_id);
        if let Some(parent) = path.parent() {
            secure_create_dir_all(parent)?;
        }
        let mut file = secure_create_file(&path)?;
        file.write_all(format!("{uid}\n{size}\n").as_bytes())?;
        file.sync_all()?;
        Ok(())
    }

    fn ensure_folder(&self, folder: &str) -> Result<(), VivariumError> {
        let folder = resolve_folder(folder)?;
        secure_create_dir_all(&self.root)?;
        let path = self.folder_path(folder);
        secure_create_dir_all(&path)?;
        for dir in MAILDIR_DIRS {
            let subdir = path.join(dir);
            secure_create_dir_all(&subdir)?;
            tracing::debug!(path = %subdir.display(), "ensured private maildir subdir");
        }
        Ok(())
    }

    fn find_message(
        &self,
        message_id: &str,
        folders: &[&str],
    ) -> Result<Option<PathBuf>, VivariumError> {
        let wanted = display_message_id(message_id);
        for folder in folders {
            if let Some(path) = self.find_message_in_subdirs(&wanted, folder, &["new", "cur"])? {
                return Ok(Some(path));
            }
        }
        Ok(None)
    }

    fn find_message_in_subdirs(
        &self,
        message_id: &str,
        folder: &str,
        subdirs: &[&str],
    ) -> Result<Option<PathBuf>, VivariumError> {
        let wanted = display_message_id(message_id);
        let path = self.folder_path(folder);
        for subdir in subdirs {
            let dir = path.join(subdir);
            if !dir.exists() {
                continue;
            }
            for entry in fs::read_dir(&dir)? {
                let entry = entry?;
                let file_path = entry.path();
                if message_id_from_path(&file_path).as_deref() == Some(wanted.as_str()) {
                    return Ok(Some(file_path));
                }
            }
        }
        Ok(None)
    }

    fn index_path(&self, folder: &str, rfc_message_id: &str) -> PathBuf {
        self.root
            .join(".vivarium_index")
            .join(canonical_folder(folder).unwrap_or(folder))
            .join(format!("{:016x}", stable_hash(rfc_message_id)))
    }
}

fn resolve_folder(folder: &str) -> Result<&'static str, VivariumError> {
    canonical_folder(folder).ok_or_else(|| {
        VivariumError::Message(format!(
            "invalid folder '{folder}', expected inbox, archive, sent, drafts, or outbox"
        ))
    })
}

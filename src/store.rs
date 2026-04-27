use std::fs;
use std::path::{Path, PathBuf};

use crate::error::VivariumError;
use crate::message::MessageEntry;

/// Local mail folders — the only buckets that exist.
const FOLDERS: &[&str] = &["inbox", "archive", "sent", "drafts", "outbox"];

/// File-based mail store for a single account.
///
/// Layout:
/// ```text
/// {root}/
/// ├── inbox/      ← messages with INBOX label / in INBOX folder
/// ├── archive/    ← everything else
/// ├── sent/       ← sent mail
/// ├── drafts/     ← work in progress
/// └── outbox/     ← queued for sending
/// ```
///
/// Each message is stored as `{message_id}.eml`.
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

    /// Create all local folders if they don't exist.
    pub fn ensure_folders(&self) -> Result<(), VivariumError> {
        for folder in FOLDERS {
            let path = self.root.join(folder);
            if !path.exists() {
                fs::create_dir_all(&path)?;
                tracing::debug!(path = %path.display(), "created folder");
            }
        }
        Ok(())
    }

    /// Path to a specific folder.
    pub fn folder_path(&self, folder: &str) -> PathBuf {
        self.root.join(folder)
    }

    /// List message entries in a folder.
    pub fn list_messages(&self, folder: &str) -> Result<Vec<MessageEntry>, VivariumError> {
        let path = self.folder_path(folder);
        if !path.exists() {
            return Ok(vec![]);
        }
        let mut entries = Vec::new();
        for entry in fs::read_dir(&path)? {
            let entry = entry?;
            let file_path = entry.path();
            if file_path.extension().is_some_and(|e| e == "eml") {
                entries.push(MessageEntry::from_path(&file_path)?);
            }
        }
        entries.sort_by(|a, b| b.date.cmp(&a.date));
        Ok(entries)
    }

    /// Read raw message bytes by message ID (looks across all folders).
    pub fn read_message(&self, message_id: &str) -> Result<Vec<u8>, VivariumError> {
        let filename = format!("{message_id}.eml");
        for folder in FOLDERS {
            let path = self.root.join(folder).join(&filename);
            if path.exists() {
                return Ok(fs::read(&path)?);
            }
        }
        Err(VivariumError::Message(format!(
            "message not found: {message_id}"
        )))
    }

    /// Store a message in a folder.
    pub fn store_message(
        &self,
        folder: &str,
        message_id: &str,
        data: &[u8],
    ) -> Result<PathBuf, VivariumError> {
        let path = self.folder_path(folder).join(format!("{message_id}.eml"));
        fs::write(&path, data)?;
        tracing::debug!(path = %path.display(), "stored message");
        Ok(path)
    }

    /// Move a message between folders (e.g. inbox → archive).
    pub fn move_message(
        &self,
        message_id: &str,
        from: &str,
        to: &str,
    ) -> Result<PathBuf, VivariumError> {
        let filename = format!("{message_id}.eml");
        let src = self.folder_path(from).join(&filename);
        let dst = self.folder_path(to).join(&filename);
        if !src.exists() {
            return Err(VivariumError::Message(format!(
                "message not found in {from}: {message_id}"
            )));
        }
        fs::rename(&src, &dst)?;
        tracing::debug!(from = %src.display(), to = %dst.display(), "moved message");
        Ok(dst)
    }

    /// Check if a message exists in any folder.
    pub fn contains(&self, message_id: &str) -> bool {
        let filename = format!("{message_id}.eml");
        FOLDERS
            .iter()
            .any(|f| self.root.join(f).join(&filename).exists())
    }

    /// Get the file size of a message in a specific folder, if it exists.
    pub fn file_size(&self, folder: &str, message_id: &str) -> Option<u64> {
        let path = self.folder_path(folder).join(format!("{message_id}.eml"));
        fs::metadata(&path).ok().map(|m| m.len())
    }

    /// Build a map of message_id → file_size for all .eml files in a folder.
    pub fn local_sizes(&self, folder: &str) -> Result<std::collections::HashMap<String, u64>, VivariumError> {
        let path = self.folder_path(folder);
        let mut map = std::collections::HashMap::new();
        if !path.exists() {
            return Ok(map);
        }
        for entry in fs::read_dir(&path)? {
            let entry = entry?;
            let file_path = entry.path();
            if file_path.extension().is_some_and(|e| e == "eml")
                && let Some(stem) = file_path.file_stem().and_then(|s| s.to_str())
            {
                let size = fs::metadata(&file_path).map(|m| m.len()).unwrap_or(0);
                map.insert(stem.to_string(), size);
            }
        }
        Ok(map)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ensure_folders_creates_dirs() {
        let tmp = tempfile::tempdir().unwrap();
        let store = MailStore::new(tmp.path());
        store.ensure_folders().unwrap();
        for folder in FOLDERS {
            assert!(tmp.path().join(folder).is_dir());
        }
    }
}

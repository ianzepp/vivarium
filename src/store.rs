use std::collections::HashMap;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use crate::error::VivariumError;
use crate::message::MessageEntry;

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
        self.root.join(canonical_folder(folder))
    }

    /// List message entries in a folder.
    pub fn list_messages(&self, folder: &str) -> Result<Vec<MessageEntry>, VivariumError> {
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
        entries.sort_by(|a, b| b.date.cmp(&a.date));
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

        let mut file = fs::File::create(&tmp_path)?;
        file.write_all(data)?;
        file.sync_all()?;
        fs::rename(&tmp_path, &final_path)?;
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
        self.ensure_folder(to)?;
        let src = self
            .find_message_in_subdirs(message_id, canonical_folder(from), MAILDIR_DIRS)?
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
        let dst = self.folder_path(to).join(subdir).join(filename);

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
        self.find_message(message_id, &[canonical_folder(folder)])
            .ok()
            .flatten()
            .and_then(|path| fs::metadata(&path).ok().map(|m| m.len()))
    }

    /// Build a map of message_id -> file_size for all message files in a folder.
    pub fn local_sizes(&self, folder: &str) -> Result<HashMap<String, u64>, VivariumError> {
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

    fn ensure_folder(&self, folder: &str) -> Result<(), VivariumError> {
        let path = self.folder_path(folder);
        for dir in MAILDIR_DIRS {
            let subdir = path.join(dir);
            if !subdir.exists() {
                fs::create_dir_all(&subdir)?;
                tracing::debug!(path = %subdir.display(), "created maildir subdir");
            }
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
}

pub fn message_id_from_path(path: &Path) -> Option<String> {
    path.file_name()
        .and_then(|n| n.to_str())
        .map(display_message_id)
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

fn maildir_filename(message_id: &str, subdir: &str) -> String {
    let base = storage_message_id(message_id);
    if subdir == "cur" {
        format!("{base}:2,S")
    } else {
        base
    }
}

fn storage_message_id(message_id: &str) -> String {
    let display = display_message_id(message_id);
    format!("{display}.eml")
}

fn display_message_id(message_id: &str) -> String {
    let before_flags = message_id
        .split_once(":2,")
        .map_or(message_id, |(id, _)| id);
    before_flags
        .strip_suffix(".eml")
        .unwrap_or(before_flags)
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ensure_folders_creates_maildirs() {
        let tmp = tempfile::tempdir().unwrap();
        let store = MailStore::new(tmp.path());
        store.ensure_folders().unwrap();
        for folder in FOLDERS {
            assert!(tmp.path().join(folder).join("new").is_dir());
            assert!(tmp.path().join(folder).join("cur").is_dir());
            assert!(tmp.path().join(folder).join("tmp").is_dir());
        }
    }

    #[test]
    fn store_message_writes_via_maildir() {
        let tmp = tempfile::tempdir().unwrap();
        let store = MailStore::new(tmp.path());

        let path = store
            .store_message("inbox", "inbox-1", b"Subject: hello\r\n\r\nbody")
            .unwrap();

        assert_eq!(path, tmp.path().join("INBOX/new/inbox-1.eml"));
        assert_eq!(
            store.read_message("inbox-1").unwrap(),
            b"Subject: hello\r\n\r\nbody"
        );
    }

    #[test]
    fn message_ids_ignore_maildir_flags() {
        let path = PathBuf::from("INBOX/cur/inbox-1.eml:2,S");
        assert_eq!(message_id_from_path(&path).unwrap(), "inbox-1");
    }

    #[test]
    fn move_message_preserves_source_subdir() {
        let tmp = tempfile::tempdir().unwrap();
        let store = MailStore::new(tmp.path());
        let src = store
            .store_message("inbox", "inbox-1", b"Subject: hello\r\n\r\nbody")
            .unwrap();

        let dst = store.move_message("inbox-1", "inbox", "archive").unwrap();

        assert_eq!(src.parent().unwrap().file_name().unwrap(), "new");
        assert_eq!(dst, tmp.path().join("Archive/new/inbox-1.eml"));
        assert!(dst.exists());
    }

    #[test]
    fn move_message_rejects_tmp_source() {
        let tmp = tempfile::tempdir().unwrap();
        let store = MailStore::new(tmp.path());
        store.ensure_folders().unwrap();
        fs::write(
            tmp.path().join("INBOX/tmp/inbox-1.eml"),
            b"Subject: hello\r\n\r\nbody",
        )
        .unwrap();

        let err = store
            .move_message("inbox-1", "inbox", "archive")
            .unwrap_err();
        assert!(err.to_string().contains("unexpected maildir subdirectory"));
    }
}

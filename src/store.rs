use std::cmp::Reverse;
use std::collections::HashMap;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};

use crate::error::VivariumError;
use crate::message::{MessageEntry, message_id_from_bytes};
use crate::storage::Storage;

mod mutate;
mod path;
mod secure;
#[cfg(test)]
mod tests;

pub use path::message_id_from_path;
use path::{canonical_folder, display_message_id, is_message_file, maildir_filename, stable_hash};
pub(crate) use secure::secure_create_dir_all;
use secure::{secure_create_file, secure_file};

/// Local staging folders retained for draft/outbox file workflows.
const FOLDERS: &[&str] = &[
    "INBOX", "Archive", "Trash", "Sent", "Drafts", "Tasks", "Done", "outbox",
];
const MAILDIR_DIRS: &[&str] = &["tmp", "new", "cur"];

/// Account-local storage facade.
///
/// Ordinary message list/read/locate paths resolve through `storage.sqlite`.
/// File-backed folders remain only for explicit local draft and outbox staging.
#[derive(Clone)]
pub struct MailStore {
    root: PathBuf,
}

#[derive(Debug, Clone)]
pub struct MessageLocation {
    pub message_id: Option<String>,
    pub local_role: String,
    pub content_id: Option<String>,
    pub path: PathBuf,
}

impl MailStore {
    #[must_use] 
    pub fn new(root: &Path) -> Self {
        Self {
            root: root.to_path_buf(),
        }
    }

    #[must_use] 
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Create all local Maildir folders if they don't exist.
    ///
    /// # Errors
    /// Returns an error if creating the directory structure fails.
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
    #[must_use] 
    pub fn folder_path(&self, folder: &str) -> PathBuf {
        self.root.join(canonical_folder(folder).unwrap_or(folder))
    }

    /// List message entries in a folder.
    ///
    /// # Errors
    /// Returns an error if reading the storage index or the outbox directory
    /// fails.
    pub fn list_messages(&self, folder: &str) -> Result<Vec<MessageEntry>, VivariumError> {
        let folder = resolve_folder(folder)?;
        if folder != "outbox" {
            let storage = Storage::open(&self.root)?;
            let stored = storage.list_messages_by_role(&storage_role(folder))?;
            let mut entries: Vec<_> = stored
                .into_iter()
                .map(|message| MessageEntry {
                    message_id: message.handle,
                    from: message.from_addr,
                    subject: message.subject,
                    date: parse_storage_date(&message.date),
                    path: self.root.join(message.blob_relpath),
                    read_state: message.read_state,
                    starred: message.starred,
                })
                .collect();
            entries.sort_by_key(|entry| Reverse(entry.date));
            return Ok(entries);
        }

        let mut entries = Vec::new();
        for file_path in self.outbox_message_paths()? {
            entries.push(MessageEntry::from_path(&file_path)?);
        }
        entries.sort_by_key(|entry| Reverse(entry.date));
        Ok(entries)
    }

    /// Read raw message bytes by message ID (looks across all folders).
    ///
    /// # Errors
    /// Returns an error if the message ID cannot be resolved or the file
    /// cannot be read.
    pub fn read_message(&self, message_id: &str) -> Result<Vec<u8>, VivariumError> {
        let storage = Storage::open(&self.root)?;
        let resolved = storage.resolve_message_token(message_id)?;
        storage.read_message(&resolved)
    }

    /// Locate a message by handle across all user-facing folders.
    ///
    /// # Errors
    /// Returns an error if the message ID cannot be resolved or the message
    /// is not found.
    pub fn locate_message(&self, message_id: &str) -> Result<MessageLocation, VivariumError> {
        let storage = Storage::open(&self.root)?;
        let resolved = storage.resolve_message_token(message_id)?;
        if let Some(message) = storage.message_by_id(&resolved)? {
            return Ok(MessageLocation {
                message_id: Some(message.message_id),
                local_role: message.local_role,
                content_id: Some(message.content_id),
                path: self.root.join(message.blob_relpath),
            });
        }
        Err(VivariumError::Message(format!(
            "message not found: {message_id}"
        )))
    }

    /// Resolve a message token (handle or ID) to a canonical message ID.
    ///
    /// # Errors
    /// Returns an error if the token cannot be resolved in storage.
    pub fn resolve_message_id(&self, token: &str) -> Result<String, VivariumError> {
        Storage::open(&self.root)?.resolve_message_token(token)
    }

    /// Display a human-readable handle for a message token.
    ///
    /// # Errors
    /// Returns an error if storage cannot be opened or the token cannot be
    /// resolved.
    pub fn display_handle(&self, token: &str) -> Result<String, VivariumError> {
        if let Ok(storage) = Storage::open(&self.root)
            && let Ok(message_id) = storage.resolve_message_token(token)
        {
            return storage.display_handle(&message_id);
        }
        Ok(display_message_id(token))
    }

    /// Store a message in `new/`.
    ///
    /// # Errors
    /// Returns an error if the folder is invalid or the file cannot be written.
    pub fn store_message(
        &self,
        folder: &str,
        message_id: &str,
        data: &[u8],
    ) -> Result<PathBuf, VivariumError> {
        self.store_message_in(folder, "new", message_id, data)
    }

    /// Store a message in a specific Maildir subdirectory.
    ///
    /// # Errors
    /// Returns an error if the folder or subdirectory is invalid, or the file
    /// cannot be written.
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

    /// Build a map of `message_id` -> `file_size` for all message files in a folder.
    ///
    /// # Errors
    /// Returns an error if reading the storage index or outbox directory fails.
    pub fn local_sizes(&self, folder: &str) -> Result<HashMap<String, u64>, VivariumError> {
        let folder = resolve_folder(folder)?;
        if folder != "outbox" {
            let storage = Storage::open(&self.root)?;
            return storage.local_sizes_by_role(&storage_role(folder));
        }
        let mut map = HashMap::new();
        for file_path in self.outbox_message_paths()? {
            if let Some(id) = message_id_from_path(&file_path) {
                let size = fs::metadata(&file_path).map_or(0, |m| m.len());
                map.insert(id, size);
            }
        }
        Ok(map)
    }

    /// Build an in-memory map of RFC 5322 Message-ID → (uid, size) for a folder.
    /// Scans every .eml file in new/ and cur/ once.
    ///
    /// # Errors
    /// Returns an error if reading the storage index or scanning files fails.
    pub fn build_rfc_index(
        &self,
        folder: &str,
    ) -> Result<HashMap<String, (u32, u64)>, VivariumError> {
        let folder = resolve_folder(folder)?;
        if folder != "outbox" {
            let storage = Storage::open(&self.root)?;
            return storage.rfc_index_by_role(&storage_role(folder));
        }
        let mut map = HashMap::new();
        for file_path in self.outbox_message_paths()? {
            let data = fs::read(&file_path)?;
            if let Some(rfc_id) = message_id_from_bytes(&data) {
                let size = fs::metadata(&file_path)?.len();
                let uid = message_id_from_path(&file_path)
                    .and_then(|id| id.rsplit_once('-').and_then(|(_, uid)| uid.parse().ok()))
                    .unwrap_or(0);
                map.insert(rfc_id, (uid, size));
            }
        }
        Ok(map)
    }

    /// Check if an RFC 5322 Message-ID exists in the index with a matching size.
    #[must_use] 
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
    #[must_use] 
    pub fn rfc_index_contains(
        &self,
        index: &HashMap<String, (u32, u64)>,
        rfc_message_id: &str,
    ) -> bool {
        index.contains_key(rfc_message_id)
    }

    /// Write an RFC message ID index entry to disk.
    ///
    /// # Errors
    /// Returns an error if creating the index directory or writing the file
    /// fails.
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

    pub(super) fn find_message_in_subdirs(
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

    fn outbox_message_paths(&self) -> Result<Vec<PathBuf>, VivariumError> {
        let path = self.folder_path("outbox");
        if !path.exists() {
            return Ok(vec![]);
        }

        let mut paths = Vec::new();
        for subdir in ["new", "cur"] {
            let dir = path.join(subdir);
            if !dir.exists() {
                continue;
            }
            for entry in fs::read_dir(&dir)? {
                let entry = entry?;
                let file_path = entry.path();
                if is_message_file(&file_path) {
                    paths.push(file_path);
                }
            }
        }
        Ok(paths)
    }
}

pub(super) fn resolve_folder(folder: &str) -> Result<&'static str, VivariumError> {
    canonical_folder(folder).ok_or_else(|| {
        VivariumError::Message(format!(
            "invalid folder '{folder}', expected inbox, archive, trash, sent, drafts, tasks, needs, wants, done, or outbox"
        ))
    })
}

fn storage_role(folder: &str) -> String {
    match folder {
        "INBOX" => "inbox".into(),
        "Archive" => "archive".into(),
        "Trash" => "trash".into(),
        "Sent" => "sent".into(),
        "Drafts" => "drafts".into(),
        "Tasks" => "tasks".into(),
        "Done" => "done".into(),
        other => other.to_ascii_lowercase(),
    }
}

fn parse_storage_date(value: &str) -> DateTime<Utc> {
    DateTime::parse_from_rfc3339(value)
        .map(|dt| dt.with_timezone(&Utc))
        .unwrap_or_default()
}

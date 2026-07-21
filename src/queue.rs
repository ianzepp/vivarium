use std::fs::{self, OpenOptions};
use std::io::Write;
#[cfg(unix)]
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};

use chrono::Utc;
use serde::{Deserialize, Serialize};

use crate::VivariumError;
use crate::store::secure_create_dir_all;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct QueueItem {
    pub id: String,
    pub created_at: String,
    pub updated_at: String,
    pub account: String,
    pub status: QueueStatus,
    pub command: QueuedCommand,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum QueueStatus {
    Pending,
    Executed,
    Failed,
    Dropped,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "operation", rename_all = "snake_case")]
pub enum QueuedCommand {
    Archive {
        handles: Vec<String>,
    },
    Delete {
        handles: Vec<String>,
        expunge: bool,
        confirm: bool,
    },
    Move {
        handle: String,
        folder: String,
    },
    Flag {
        handle: String,
        read: bool,
        unread: bool,
        star: bool,
        unstar: bool,
    },
    Send {
        path: PathBuf,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        from: Option<String>,
    },
    Reply {
        handle: String,
        body: String,
    },
}

impl QueueItem {
    #[must_use]
    pub fn new(account: String, command: QueuedCommand) -> Self {
        let now = Utc::now();
        let timestamp = now.to_rfc3339();
        Self {
            id: format!("q{}", now.timestamp_nanos_opt().unwrap_or_default()),
            created_at: timestamp.clone(),
            updated_at: timestamp,
            account,
            status: QueueStatus::Pending,
            command,
            error: None,
        }
    }

    pub fn mark(&mut self, status: QueueStatus, error: Option<String>) {
        self.status = status;
        self.error = error;
        self.updated_at = Utc::now().to_rfc3339();
    }
}

/// Enqueue a queue item to disk.
///
/// # Errors
/// Returns an error if creating the queue directory or writing the item fails.
pub fn enqueue(mail_root: &Path, item: &QueueItem) -> Result<PathBuf, VivariumError> {
    let path = item_path(mail_root, &item.id);
    if let Some(parent) = path.parent() {
        secure_create_dir_all(parent)?;
    }
    write_item(&path, item)?;
    Ok(path)
}

/// Load a queue item from disk by ID.
///
/// # Errors
/// Returns an error if the item file cannot be read or parsed.
pub fn load(mail_root: &Path, id: &str) -> Result<QueueItem, VivariumError> {
    let path = item_path(mail_root, id);
    let data = fs::read_to_string(&path)
        .map_err(|e| VivariumError::Other(format!("queued item '{id}' not found: {e}")))?;
    serde_json::from_str(&data)
        .map_err(|e| VivariumError::Parse(format!("queued item '{id}' is invalid: {e}")))
}

/// Save an updated queue item to disk.
///
/// # Errors
/// Returns an error if writing the item file fails.
pub fn save(mail_root: &Path, item: &QueueItem) -> Result<(), VivariumError> {
    write_item(&item_path(mail_root, &item.id), item)
}

/// List queue items, optionally including non-pending items.
///
/// # Errors
/// Returns an error if reading the queue directory or parsing an item fails.
pub fn list(mail_root: &Path, include_all: bool) -> Result<Vec<QueueItem>, VivariumError> {
    let dir = queue_dir(mail_root);
    if !dir.exists() {
        return Ok(Vec::new());
    }
    let mut items = Vec::new();
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        if entry.path().extension().and_then(|ext| ext.to_str()) != Some("json") {
            continue;
        }
        let data = fs::read_to_string(entry.path())?;
        let item: QueueItem = serde_json::from_str(&data)
            .map_err(|e| VivariumError::Parse(format!("invalid queue item: {e}")))?;
        if include_all || item.status == QueueStatus::Pending {
            items.push(item);
        }
    }
    items.sort_by(|a, b| {
        a.created_at
            .cmp(&b.created_at)
            .then_with(|| a.id.cmp(&b.id))
    });
    Ok(items)
}

/// List all pending queue item IDs.
///
/// # Errors
/// Returns an error if listing queue items fails.
pub fn pending_ids(mail_root: &Path) -> Result<Vec<String>, VivariumError> {
    Ok(list(mail_root, false)?
        .into_iter()
        .map(|item| item.id)
        .collect())
}

fn write_item(path: &Path, item: &QueueItem) -> Result<(), VivariumError> {
    let mut options = OpenOptions::new();
    options.create(true).truncate(true).write(true);
    #[cfg(unix)]
    options.mode(0o600);
    let mut file = options.open(path)?;
    let json = serde_json::to_string_pretty(item)
        .map_err(|e| VivariumError::Other(format!("queue serialization failed: {e}")))?;
    file.write_all(json.as_bytes())?;
    file.write_all(b"\n")?;
    file.sync_all()?;
    Ok(())
}

fn item_path(mail_root: &Path, id: &str) -> PathBuf {
    queue_dir(mail_root).join(format!("{id}.json"))
}

fn queue_dir(mail_root: &Path) -> PathBuf {
    mail_root.join(".vivarium").join("queue")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn enqueue_round_trips_pending_item() {
        let tmp = tempfile::tempdir().unwrap();
        let item = QueueItem::new(
            "acct".into(),
            QueuedCommand::Archive {
                handles: vec!["one".into()],
            },
        );

        enqueue(tmp.path(), &item).unwrap();
        let loaded = load(tmp.path(), &item.id).unwrap();

        assert_eq!(loaded.status, QueueStatus::Pending);
        assert_eq!(loaded.command, item.command);
    }

    #[test]
    fn list_hides_non_pending_by_default() {
        let tmp = tempfile::tempdir().unwrap();
        let pending = QueueItem::new(
            "acct".into(),
            QueuedCommand::Archive {
                handles: vec!["one".into()],
            },
        );
        let mut executed = QueueItem::new(
            "acct".into(),
            QueuedCommand::Archive {
                handles: vec!["two".into()],
            },
        );
        executed.mark(QueueStatus::Executed, None);

        enqueue(tmp.path(), &pending).unwrap();
        enqueue(tmp.path(), &executed).unwrap();

        assert_eq!(list(tmp.path(), false).unwrap().len(), 1);
        assert_eq!(list(tmp.path(), true).unwrap().len(), 2);
    }
}

use std::fs::OpenOptions;
use std::io::Write;
#[cfg(unix)]
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};

use chrono::Utc;
use serde::Serialize;

use super::PreparedMutation;
use crate::error::VivariumError;
use crate::store::secure_create_dir_all;

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct MutationAuditRecord {
    pub timestamp: String,
    pub account: String,
    pub handle: String,
    pub operation: String,
    pub status: String,
    pub dry_run: bool,
    pub source_mailbox: String,
    pub target_mailbox: Option<String>,
    pub uid: u32,
    pub uidvalidity: u32,
    pub command_path: String,
    pub error: Option<String>,
}

pub fn append_audit(
    mail_root: &Path,
    prepared: &PreparedMutation,
    status: &str,
    dry_run: bool,
    error: Option<String>,
) -> Result<PathBuf, VivariumError> {
    let path = audit_path(mail_root);
    if let Some(parent) = path.parent() {
        secure_create_dir_all(parent)?;
    }
    let mut options = OpenOptions::new();
    options.create(true).append(true).write(true);
    #[cfg(unix)]
    options.mode(0o600);
    let mut file = options.open(&path)?;
    let record = audit_record(prepared, status, dry_run, error);
    let line = serde_json::to_string(&record)
        .map_err(|e| VivariumError::Other(format!("audit serialization failed: {e}")))?;
    file.write_all(line.as_bytes())?;
    file.write_all(b"\n")?;
    file.sync_all()?;
    Ok(path)
}

pub fn audit_record(
    prepared: &PreparedMutation,
    status: &str,
    dry_run: bool,
    error: Option<String>,
) -> MutationAuditRecord {
    MutationAuditRecord {
        timestamp: Utc::now().to_rfc3339(),
        account: prepared.preview.account.clone(),
        handle: prepared.preview.handle.clone(),
        operation: prepared.preview.operation.clone(),
        status: status.into(),
        dry_run,
        source_mailbox: prepared.preview.source_mailbox.clone(),
        target_mailbox: prepared.preview.target_mailbox.clone(),
        uid: prepared.preview.uid,
        uidvalidity: prepared.preview.uidvalidity,
        command_path: prepared.preview.command_path.clone(),
        error,
    }
}

fn audit_path(mail_root: &Path) -> PathBuf {
    mail_root
        .join(".vivarium")
        .join("audit")
        .join("mutations.jsonl")
}

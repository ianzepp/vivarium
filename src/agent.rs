use std::fs::OpenOptions;
use std::io::Write;
#[cfg(unix)]
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};

use chrono::Utc;
use serde::Serialize;

use crate::error::VivariumError;
use crate::store::secure_create_dir_all;

pub const DEFAULT_MAX_BODY_BYTES: usize = 4096;
pub const DEFAULT_MAX_RESULTS: usize = 20;

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct AgentAuditRecord {
    pub timestamp: String,
    pub account: String,
    pub operation: String,
    pub status: String,
    pub target: String,
    pub external_write: bool,
    pub execute: bool,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct BoundedText {
    pub text: String,
    pub truncated: bool,
    pub max_bytes: usize,
}

pub fn plan_json(
    account: &str,
    operation: &str,
    target: &str,
    external_write: bool,
    execute: bool,
    preview: serde_json::Value,
) -> serde_json::Value {
    serde_json::json!({
        "status": if execute { "approved" } else { "planned" },
        "account": account,
        "operation": operation,
        "target": target,
        "external_write": external_write,
        "approval_required": external_write && !execute,
        "execute": execute,
        "preview": preview,
    })
}

pub fn bounded_text(text: &str, max_bytes: usize) -> BoundedText {
    if text.len() <= max_bytes {
        return BoundedText {
            text: text.into(),
            truncated: false,
            max_bytes,
        };
    }
    let mut end = max_bytes;
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    BoundedText {
        text: text[..end].into(),
        truncated: true,
        max_bytes,
    }
}

pub fn append_audit(mail_root: &Path, record: AgentAuditRecord) -> Result<PathBuf, VivariumError> {
    let path = audit_path(mail_root);
    if let Some(parent) = path.parent() {
        secure_create_dir_all(parent)?;
    }
    let mut options = OpenOptions::new();
    options.create(true).append(true).write(true);
    #[cfg(unix)]
    options.mode(0o600);
    let mut file = options.open(&path)?;
    let line = serde_json::to_string(&record)
        .map_err(|e| VivariumError::Other(format!("agent audit serialization failed: {e}")))?;
    file.write_all(line.as_bytes())?;
    file.write_all(b"\n")?;
    file.sync_all()?;
    Ok(path)
}

pub fn audit_record(
    account: &str,
    operation: &str,
    status: &str,
    target: &str,
    external_write: bool,
    execute: bool,
    error: Option<String>,
) -> AgentAuditRecord {
    AgentAuditRecord {
        timestamp: Utc::now().to_rfc3339(),
        account: account.into(),
        operation: operation.into(),
        status: status.into(),
        target: target.into(),
        external_write,
        execute,
        error,
    }
}

fn audit_path(mail_root: &Path) -> PathBuf {
    mail_root
        .join(".vivarium")
        .join("audit")
        .join("agent.jsonl")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bounded_text_truncates_at_character_boundary() {
        let bounded = bounded_text("abcdéfg", 5);

        assert_eq!(bounded.text, "abcd");
        assert!(bounded.truncated);
    }

    #[test]
    fn audit_record_preserves_approval_state() {
        let record = audit_record("acct", "send", "planned", "draft.eml", true, false, None);

        assert_eq!(record.operation, "send");
        assert!(record.external_write);
        assert!(!record.execute);
    }
}

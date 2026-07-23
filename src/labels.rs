use serde::Serialize;

use crate::config::{Account, Provider};
use crate::error::VivariumError;

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct LabelSupport {
    pub account: String,
    pub provider: String,
    pub label_roots: Vec<String>,
    pub mutation_supported: bool,
    pub mode: String,
    pub reason: String,
    pub safe_alternative: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LabelOperation {
    Add,
    Remove,
}

#[must_use]
pub fn support(account: &Account) -> LabelSupport {
    let label_roots = account.label_roots();
    let (mode, reason) = match account.provider {
        Provider::ProtonApi => (
            "direct_proton_api_unsupported",
            "Direct Proton API label mutation is not implemented.",
        ),
        Provider::Protonmail => (
            "folder_moves_only",
            "Proton Bridge exposes the safe Vivi surface as IMAP folders; independent label mutation is not implemented.",
        ),
        Provider::Gmail => (
            "gmail_labels_scoped",
            "Gmail labels require provider-specific X-GM-LABELS support; Vivi does not mutate them yet.",
        ),
        Provider::Standard => (
            "standard_imap_folders_only",
            "Standard IMAP has folders and flags, not portable labels.",
        ),
    };
    LabelSupport {
        account: account.name.clone(),
        provider: account.provider.to_string(),
        label_roots,
        mutation_supported: false,
        mode: mode.into(),
        reason: reason.into(),
        safe_alternative: "Use `vivi exec move <handle> <folder>` for supported folder roles."
            .into(),
    }
}

#[must_use]
pub fn plan_json(
    account: &Account,
    handle: &str,
    operation: &LabelOperation,
    label: &str,
    dry_run: bool,
) -> serde_json::Value {
    let support = support(account);
    serde_json::json!({
        "status": "unsupported",
        "dry_run": dry_run,
        "account": account.name,
        "provider": account.provider.to_string(),
        "handle": handle,
        "operation": operation_name(operation),
        "label": label,
        "support": support,
    })
}

#[must_use]
pub fn unsupported_error(account: &Account, label: &str) -> VivariumError {
    let support = support(account);
    VivariumError::Message(format!(
        "label mutation unsupported for provider {} on account '{}': {}; label '{}'",
        support.provider, support.account, support.reason, label
    ))
}

#[must_use]
pub fn operation_name(operation: &LabelOperation) -> &'static str {
    match operation {
        LabelOperation::Add => "label_add",
        LabelOperation::Remove => "label_remove",
    }
}

#[cfg(test)]
#[path = "labels_test.rs"]
mod tests;

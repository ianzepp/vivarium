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

pub fn support(account: &Account) -> LabelSupport {
    let label_roots = account.label_roots();
    let (mode, reason) = match account.provider {
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
        safe_alternative: "Use `vivi move <handle> <folder>` for supported folder roles.".into(),
    }
}

pub fn plan_json(
    account: &Account,
    handle: &str,
    operation: LabelOperation,
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
        "operation": operation_name(&operation),
        "label": label,
        "support": support,
    })
}

pub fn unsupported_error(account: &Account, label: &str) -> VivariumError {
    let support = support(account);
    VivariumError::Message(format!(
        "label mutation unsupported for provider {} on account '{}': {}; label '{}'",
        support.provider, support.account, support.reason, label
    ))
}

pub fn operation_name(operation: &LabelOperation) -> &'static str {
    match operation {
        LabelOperation::Add => "label_add",
        LabelOperation::Remove => "label_remove",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{Auth, Security};

    #[test]
    fn protonmail_reports_folder_only_labels() {
        let account = account(Provider::Protonmail);
        let support = support(&account);

        assert_eq!(support.mode, "folder_moves_only");
        assert!(!support.mutation_supported);
    }

    #[test]
    fn gmail_is_scoped_separately_from_standard_imap() {
        let gmail = support(&account(Provider::Gmail));
        let standard = support(&account(Provider::Standard));

        assert_eq!(gmail.mode, "gmail_labels_scoped");
        assert_eq!(standard.mode, "standard_imap_folders_only");
    }

    #[test]
    fn unsupported_plan_names_operation_and_label() {
        let account = account(Provider::Standard);
        let json = plan_json(&account, "handle-1", LabelOperation::Add, "Work", true);

        assert_eq!(json["status"], "unsupported");
        assert_eq!(json["operation"], "label_add");
        assert_eq!(json["label"], "Work");
    }

    fn account(provider: Provider) -> Account {
        Account {
            name: "acct".into(),
            email: "acct@example.com".into(),
            imap_host: "localhost".into(),
            imap_port: Some(1143),
            imap_security: Security::Starttls,
            smtp_host: "localhost".into(),
            smtp_port: Some(1025),
            smtp_security: Security::Starttls,
            username: "acct@example.com".into(),
            auth: Auth::Password,
            password: Some("secret".into()),
            password_cmd: None,
            token_cmd: None,
            oauth_client_id: None,
            oauth_client_secret: None,
            mail_dir: None,
            inbox_folder: None,
            archive_folder: None,
            trash_folder: None,
            sent_folder: None,
            drafts_folder: None,
            label_roots: Some(vec!["Labels".into()]),
            provider,
            oauth_authorization_url: None,
            oauth_token_url: None,
            oauth_scope: None,
            reject_invalid_certs: None,
        }
    }
}

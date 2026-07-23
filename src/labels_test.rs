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
    let json = plan_json(&account, "handle-1", &LabelOperation::Add, "Work", true);

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
        imap_security: Some(Security::Starttls),
        smtp_host: "localhost".into(),
        smtp_port: Some(1025),
        smtp_security: Some(Security::Starttls),
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
        storage_mode: None,
        provider,
        oauth_authorization_url: None,
        oauth_token_url: None,
        oauth_scope: None,
        reject_invalid_certs: None,
        policy: crate::config::MutationPolicy::FullWrite,
    }
}

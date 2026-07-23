use super::*;
use crate::config::{Auth, Provider, Security};

#[test]
fn resolves_provider_folder_defaults() {
    let proton = account(Provider::Protonmail);
    let gmail = account(Provider::Gmail);
    let standard = account(Provider::Standard);

    assert_eq!(resolved_folders(&proton).archive, "Archive");
    assert_eq!(resolved_folders(&proton).trash, "Trash");
    assert_eq!(resolved_folders(&gmail).archive, "Archive");
    assert_eq!(resolved_folders(&gmail).sent, "[Gmail]/Sent Mail");
    assert_eq!(resolved_folders(&standard).archive, "Archive");
}

#[test]
fn resolves_account_folder_overrides() {
    let mut account = account(Provider::Standard);
    account.archive_folder = Some("Archive".into());
    account.trash_folder = Some("Deleted Messages".into());
    account.label_roots = Some(vec!["Labels".into()]);

    let resolved = resolved_folders(&account);

    assert_eq!(resolved.archive, "Archive");
    assert_eq!(resolved.trash, "Deleted Messages");
    assert_eq!(resolved.label_roots, vec!["Labels"]);
}

fn account(provider: Provider) -> Account {
    Account {
        name: "test".into(),
        email: "test@example.com".into(),
        imap_host: "localhost".into(),
        imap_port: Some(1143),
        imap_security: Some(Security::Starttls),
        smtp_host: "localhost".into(),
        smtp_port: Some(1025),
        smtp_security: Some(Security::Starttls),
        username: "test@example.com".into(),
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
        label_roots: None,
        storage_mode: None,
        provider,
        oauth_authorization_url: None,
        oauth_token_url: None,
        oauth_scope: None,
        reject_invalid_certs: None,
        policy: crate::config::MutationPolicy::FullWrite,
    }
}

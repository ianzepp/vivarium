use notify::event::{CreateKind, DataChange, ModifyKind};

use super::*;
use crate::config::{Auth, MutationPolicy, Provider, Security};

fn test_account_with_policy(policy: MutationPolicy) -> Account {
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
        provider: Provider::Standard,
        oauth_authorization_url: None,
        oauth_token_url: None,
        oauth_scope: None,
        reject_invalid_certs: None,
        policy,
    }
}

#[test]
fn dispatchable_paths_filters_eml_creates_and_modifies() {
    let event = Event {
        kind: EventKind::Create(CreateKind::File),
        paths: vec![PathBuf::from("a.eml"), PathBuf::from("a.txt")],
        attrs: Default::default(),
    };
    assert_eq!(dispatchable_paths(&event), vec![PathBuf::from("a.eml")]);

    let event = Event {
        kind: EventKind::Modify(ModifyKind::Data(DataChange::Content)),
        paths: vec![PathBuf::from("b.eml")],
        attrs: Default::default(),
    };
    assert_eq!(dispatchable_paths(&event), vec![PathBuf::from("b.eml")]);
}

#[test]
fn claim_for_processing_moves_to_tmp() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("outbox/new/message.eml");
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(&path, b"Subject: hello\r\n\r\nbody").unwrap();

    let claimed = claim_for_processing(&path).unwrap();

    assert_eq!(
        claimed,
        tmp.path().join("outbox/tmp/message.eml.processing")
    );
    assert!(!path.exists());
    assert!(claimed.exists());
}

#[tokio::test]
async fn process_entry_denies_send_under_read_only_policy() {
    let tmp = tempfile::tempdir().unwrap();
    let store = MailStore::new(tmp.path());
    let acct = test_account_with_policy(MutationPolicy::ReadOnly);

    // Create a valid .eml in the outbox.
    let outbox_new = tmp.path().join("outbox/new");
    fs::create_dir_all(&outbox_new).unwrap();
    let path = outbox_new.join("msg.eml");
    fs::write(&path, b"Subject: hi\r\n\r\nbody").unwrap();

    // The policy check must fire before any claim or SMTP/network call.
    let err = process_entry(&acct, &store, &path, false)
        .await
        .unwrap_err();
    assert!(err.to_string().contains("policy"));
    assert!(err.to_string().contains("send"));

    // File must remain in new/ — not claimed to tmp/ — so the user can
    // find and remove it rather than having it orphaned.
    assert!(path.exists(), "denied send must leave file in new/");
    let tmp_dir = tmp.path().join("outbox/tmp");
    assert!(
        !tmp_dir.exists() || fs::read_dir(&tmp_dir).unwrap().next().is_none(),
        "no processing files should be orphaned in tmp/"
    );
}

#[tokio::test]
async fn process_entry_denies_send_under_archive_policy() {
    let tmp = tempfile::tempdir().unwrap();
    let store = MailStore::new(tmp.path());
    let acct = test_account_with_policy(MutationPolicy::Archive);

    let outbox_new = tmp.path().join("outbox/new");
    fs::create_dir_all(&outbox_new).unwrap();
    let path = outbox_new.join("msg.eml");
    fs::write(&path, b"Subject: hi\r\n\r\nbody").unwrap();

    let err = process_entry(&acct, &store, &path, false)
        .await
        .unwrap_err();
    assert!(err.to_string().contains("policy"));

    // File must remain in new/ under denial.
    assert!(path.exists());
}

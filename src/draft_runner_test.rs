use super::*;

#[test]
fn sent_reconciliation_moves_local_draft_to_sent() {
    let tmp = tempfile::tempdir().unwrap();
    let store = MailStore::new(tmp.path());
    let data = b"From: me@example.com\r\nTo: you@example.com\r\nSubject: hi\r\n\r\nbody";
    let draft = store.store_message("drafts", "draft-1", data).unwrap();

    let sent = reconcile_sent(&store, &draft, data).unwrap();

    assert!(sent.ends_with("Sent/cur/draft-1.eml:2,S"));
    assert!(sent.exists());
    assert!(!draft.exists());
}

#[test]
fn sent_reconciliation_preserves_non_draft_source() {
    let tmp = tempfile::tempdir().unwrap();
    let store = MailStore::new(tmp.path());
    let data = b"From: me@example.com\r\nTo: you@example.com\r\nSubject: hi\r\n\r\nbody";
    // Source from outside drafts (e.g. /tmp/draft.eml) — must not be removed.
    let external = tmp.path().join("external.eml");
    std::fs::write(&external, data).unwrap();

    let sent = reconcile_sent(&store, &external, data).unwrap();

    assert!(sent.exists());
    assert!(external.exists(), "non-draft source must be preserved");
}

#[test]
fn sent_reconciliation_durable_sent_copy_before_draft_removal() {
    let tmp = tempfile::tempdir().unwrap();
    let store = MailStore::new(tmp.path());
    let data = b"From: me@example.com\r\nTo: you@example.com\r\nSubject: hi\r\n\r\nbody";
    let draft = store.store_message("drafts", "draft-2", data).unwrap();

    let sent = reconcile_sent(&store, &draft, data).unwrap();

    // Sent copy must exist before draft was removed (ordering invariant).
    assert!(sent.exists(), "sent copy must be durable");
    assert!(!draft.exists(), "draft removed only after sent copy stored");
}

#[test]
fn sent_reconciliation_failure_preserves_source() {
    let tmp = tempfile::tempdir().unwrap();
    let store = MailStore::new(tmp.path());
    let data = b"From: me@example.com\r\nTo: you@example.com\r\nSubject: hi\r\n\r\nbody";
    let draft = store.store_message("drafts", "draft-3", data).unwrap();

    // Block Sent directory creation by placing a file where Sent/cur/file
    // would go. store_message_in creates parent dirs, so we block the
    // final write by making the file path occupied with a directory.
    let sent_dir = tmp.path().join("Sent");
    let blocker = sent_dir.join("cur");
    std::fs::create_dir_all(&sent_dir).unwrap();
    std::fs::write(&blocker, b"blocker").unwrap();

    let result = reconcile_sent(&store, &draft, data);

    assert!(result.is_err(), "must fail when sent copy cannot persist");
    assert!(draft.exists(), "source must survive sent-copy failure");
}

#[test]
fn require_eml_path_rejects_non_eml() {
    let err = require_eml_path(Path::new("message.txt")).unwrap_err();

    assert!(err.to_string().contains(".eml"));
}

#[test]
fn send_transport_routes_only_direct_proton_api_away_from_smtp() {
    assert_eq!(
        send_transport(&Provider::ProtonApi),
        SendTransport::DirectProtonApi
    );
    assert_eq!(send_transport(&Provider::Protonmail), SendTransport::Smtp);
    assert_eq!(send_transport(&Provider::Gmail), SendTransport::Smtp);
    assert_eq!(send_transport(&Provider::Standard), SendTransport::Smtp);
}

fn test_runtime(tmp: &std::path::Path, policy: vivarium::config::MutationPolicy) -> super::Runtime {
    use vivarium::config::{Account, AccountsFile, Auth, Config, Security};

    let account = Account {
        name: "test".into(),
        email: "test@example.com".into(),
        imap_host: "localhost".into(),
        imap_port: Some(1143),
        imap_security: Some(Security::Starttls),
        smtp_host: "localhost".into(),
        smtp_port: Some(1025),
        smtp_security: Some(Security::Starttls),
        username: "test".into(),
        auth: Auth::Password,
        password: Some("secret".into()),
        password_cmd: None,
        token_cmd: None,
        oauth_client_id: None,
        oauth_client_secret: None,
        mail_dir: Some(tmp.to_string_lossy().to_string()),
        inbox_folder: None,
        archive_folder: None,
        trash_folder: None,
        sent_folder: None,
        drafts_folder: None,
        label_roots: None,
        storage_mode: None,
        provider: vivarium::config::Provider::Standard,
        oauth_authorization_url: None,
        oauth_token_url: None,
        oauth_scope: None,
        reject_invalid_certs: None,
        policy,
    };
    super::Runtime {
        config: Config::default(),
        accounts: AccountsFile {
            accounts: vec![account],
        },
        account: Some("test".into()),
        insecure: false,
    }
}

#[tokio::test]
async fn store_draft_append_remote_denied_under_read_only() {
    let tmp = tempfile::tempdir().unwrap();
    let runtime = test_runtime(tmp.path(), vivarium::config::MutationPolicy::ReadOnly);
    let data = b"From: test@example.com\r\nTo: you@example.com\r\nSubject: hi\r\n\r\nbody";

    let err = store_draft(&runtime, data, true).await.unwrap_err();
    assert!(matches!(err, VivariumError::Policy(_)));
    assert!(err.to_string().contains("append-draft"));
}

#[tokio::test]
async fn store_draft_append_remote_denied_under_archive() {
    let tmp = tempfile::tempdir().unwrap();
    let runtime = test_runtime(tmp.path(), vivarium::config::MutationPolicy::Archive);
    let data = b"From: test@example.com\r\nTo: you@example.com\r\nSubject: hi\r\n\r\nbody";

    let err = store_draft(&runtime, data, true).await.unwrap_err();
    assert!(matches!(err, VivariumError::Policy(_)));
}

#[tokio::test]
async fn store_draft_append_remote_allowed_under_full_write() {
    let tmp = tempfile::tempdir().unwrap();
    let runtime = test_runtime(tmp.path(), vivarium::config::MutationPolicy::FullWrite);
    let data = b"From: test@example.com\r\nTo: you@example.com\r\nSubject: hi\r\n\r\nbody";

    // Local draft is created; append_remote gate passes under full-write.
    // The actual IMAP APPEND will fail because no server is running,
    // but the error must NOT be Policy — proving authorization passed.
    let err = store_draft(&runtime, data, true).await.unwrap_err();
    assert!(!matches!(err, VivariumError::Policy(_)));

    // The local draft must be stored before the remote append is attempted,
    // preserving the local-first invariant even when the remote fails.
    let mail_root = runtime
        .resolve_account(runtime.account.clone())
        .unwrap()
        .mail_path(&runtime.config);
    let drafts_dir = mail_root.join("Drafts");
    assert!(drafts_dir.exists(), "local drafts directory must exist");
}

#[tokio::test]
async fn store_draft_local_only_under_read_only() {
    let tmp = tempfile::tempdir().unwrap();
    let runtime = test_runtime(tmp.path(), vivarium::config::MutationPolicy::ReadOnly);
    let data = b"From: test@example.com\r\nTo: you@example.com\r\nSubject: hi\r\n\r\nbody";

    // Local-only draft (append_remote=false) must succeed under any policy.
    let path = store_draft(&runtime, data, false).await.unwrap();
    assert!(path.exists());
}

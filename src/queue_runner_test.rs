use super::*;
use vivarium::queue::{self, QueueItem, QueueStatus, QueuedCommand};
use vivarium::VivariumError;

fn test_runtime(tmp: &std::path::Path, policy: MutationPolicy) -> Runtime {
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
    Runtime {
        config: Config::default(),
        accounts: AccountsFile {
            accounts: vec![account],
        },
        account: Some("test".into()),
        insecure: false,
    }
}

#[tokio::test]
async fn queue_run_rejects_stale_delete_under_read_only() {
    let tmp = tempfile::tempdir().unwrap();
    let runtime = test_runtime(tmp.path(), MutationPolicy::ReadOnly);

    let stale = QueueItem::new(
        "test".into(),
        QueuedCommand::Delete {
            handles: vec!["old".into()],
            expunge: false,
            confirm: true,
        },
    );
    let mail_root = runtime
        .resolve_account(runtime.account.clone())
        .unwrap()
        .mail_path(&runtime.config);
    queue::enqueue(&mail_root, &stale).unwrap();

    let err = runtime
        .queue_run(vec![stale.id.clone()], false)
        .await
        .unwrap_err();
    assert!(err.to_string().contains("policy"));
    assert!(err.to_string().contains("read-only"));

    let loaded = queue::load(&mail_root, &stale.id).unwrap();
    assert_eq!(loaded.status, QueueStatus::Failed);
    assert!(loaded.error.unwrap().contains("policy"));
}

#[tokio::test]
async fn queue_run_rejects_stale_send_under_archive() {
    let tmp = tempfile::tempdir().unwrap();
    let runtime = test_runtime(tmp.path(), MutationPolicy::Archive);

    let stale = QueueItem::new(
        "test".into(),
        QueuedCommand::Send {
            path: std::path::PathBuf::from("old.eml"),
            from: None,
        },
    );
    let mail_root = runtime
        .resolve_account(runtime.account.clone())
        .unwrap()
        .mail_path(&runtime.config);
    queue::enqueue(&mail_root, &stale).unwrap();

    let err = runtime
        .queue_run(vec![stale.id.clone()], false)
        .await
        .unwrap_err();
    assert!(err.to_string().contains("policy"));
    assert!(err.to_string().contains("send"));

    let loaded = queue::load(&mail_root, &stale.id).unwrap();
    assert_eq!(loaded.status, QueueStatus::Failed);
}

#[tokio::test]
async fn execute_queued_rejects_delete_under_read_only() {
    let tmp = tempfile::tempdir().unwrap();
    let runtime = test_runtime(tmp.path(), MutationPolicy::ReadOnly);

    let cmd = QueuedCommand::Delete {
        handles: vec!["h1".into()],
        expunge: false,
        confirm: true,
    };
    let err = runtime.execute_queued(cmd, false).await.unwrap_err();
    assert!(matches!(err, VivariumError::Policy(_)));
    assert!(err.to_string().contains("read-only"));
}

#[tokio::test]
async fn execute_queued_rejects_send_under_read_only() {
    let tmp = tempfile::tempdir().unwrap();
    let runtime = test_runtime(tmp.path(), MutationPolicy::ReadOnly);

    let cmd = QueuedCommand::Send {
        path: std::path::PathBuf::from("test.eml"),
        from: None,
    };
    let err = runtime.execute_queued(cmd, false).await.unwrap_err();
    assert!(matches!(err, VivariumError::Policy(_)));
}

#[test]
fn enqueue_admission_rejects_delete_under_read_only() {
    let tmp = tempfile::tempdir().unwrap();
    let runtime = test_runtime(tmp.path(), MutationPolicy::ReadOnly);

    let err = runtime
        .enqueue(EnqueueCommand::Delete {
            handles: vec!["h1".into()],
            trash: false,
            expunge: false,
            confirm: false,
        })
        .unwrap_err();
    assert!(matches!(err, VivariumError::Policy(_)));

    // Verify nothing was persisted to disk.
    let mail_root = runtime
        .resolve_account(runtime.account.clone())
        .unwrap()
        .mail_path(&runtime.config);
    assert!(queue::pending_ids(&mail_root).unwrap().is_empty());
}

#[test]
fn enqueue_admission_allows_archive_under_full_write() {
    let tmp = tempfile::tempdir().unwrap();
    let runtime = test_runtime(tmp.path(), MutationPolicy::FullWrite);

    runtime
        .enqueue(EnqueueCommand::Archive {
            handles: vec!["h1".into()],
        })
        .unwrap();

    let mail_root = runtime
        .resolve_account(runtime.account.clone())
        .unwrap()
        .mail_path(&runtime.config);
    assert_eq!(queue::pending_ids(&mail_root).unwrap().len(), 1);
}

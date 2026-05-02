use super::*;
use crate::catalog::{Catalog, RemoteIdentity, handle_from_bytes};
use crate::config::{Account, Auth, Provider, Security};

#[test]
fn dry_run_plan_resolves_maildir_id_and_serializes_json() {
    let tmp = tempfile::tempdir().unwrap();
    let (_store, account, _handle) = fixture(tmp.path());
    let caps = capabilities(true, true);

    let prepared = prepare_mutation(
        &account,
        tmp.path(),
        "inbox-42",
        MutationAction::Archive,
        &caps,
        true,
    )
    .unwrap();
    let json = output_json(&prepared.preview, "planned", None);
    let audit = audit_record(&prepared, "planned", true, None);

    assert_eq!(prepared.preview.target_mailbox.as_deref(), Some("Archive"));
    assert_eq!(prepared.preview.command_path, "UID MOVE");
    assert_eq!(json["status"], "planned");
    assert_eq!(json["plan"]["local_message_id"], "inbox-42");
    assert_eq!(audit.operation, "archive");
    assert!(audit.dry_run);
}

#[test]
fn move_plan_rejects_internal_all_mail_folder_name() {
    let tmp = tempfile::tempdir().unwrap();
    let (_store, account, handle) = fixture(tmp.path());
    let err = prepare_mutation(
        &account,
        tmp.path(),
        &handle,
        MutationAction::Move {
            folder: "All Mail".into(),
        },
        &capabilities(true, true),
        true,
    )
    .unwrap_err();

    assert!(err.to_string().contains("unsupported local mirror folder"));
}

#[test]
fn reconcile_move_updates_local_maildir_and_catalog() {
    let tmp = tempfile::tempdir().unwrap();
    let (_store, account, handle) = fixture(tmp.path());
    let prepared = prepare_mutation(
        &account,
        tmp.path(),
        &handle,
        MutationAction::Archive,
        &capabilities(true, true),
        false,
    )
    .unwrap();

    let local = reconcile_success(tmp.path(), &prepared).unwrap();
    let catalog = Catalog::open(tmp.path()).unwrap();
    let entry = catalog.entry("acct", &handle).unwrap();

    assert_eq!(local.action, "move_local_copy");
    assert_eq!(entry.folder, "Archive");
    assert!(entry.raw_path.ends_with("Archive/new/inbox-42.eml"));
    assert!(entry.remote.is_none());
}

#[test]
fn reconcile_flag_updates_maildir_flags_and_keeps_remote_identity() {
    let tmp = tempfile::tempdir().unwrap();
    let (_store, account, handle) = fixture(tmp.path());
    let prepared = prepare_mutation(
        &account,
        tmp.path(),
        &handle,
        MutationAction::Flag {
            mutation: FlagMutation::Read,
        },
        &capabilities(true, true),
        false,
    )
    .unwrap();

    let local = reconcile_success(tmp.path(), &prepared).unwrap();
    let catalog = Catalog::open(tmp.path()).unwrap();
    let entry = catalog.entry("acct", &handle).unwrap();

    assert_eq!(local.maildir_subdir.as_deref(), Some("cur"));
    assert!(entry.raw_path.ends_with("INBOX/cur/inbox-42.eml:2,S"));
    assert!(entry.remote.is_some());
}

#[test]
fn reconcile_expunge_removes_local_copy_and_catalog_entry() {
    let tmp = tempfile::tempdir().unwrap();
    let (_store, account, handle) = fixture(tmp.path());
    let prepared = prepare_mutation(
        &account,
        tmp.path(),
        &handle,
        MutationAction::Expunge,
        &capabilities(false, true),
        false,
    )
    .unwrap();

    let local = reconcile_success(tmp.path(), &prepared).unwrap();
    let catalog = Catalog::open(tmp.path()).unwrap();

    assert_eq!(local.action, "remove_local_copy");
    assert!(catalog.entry("acct", &handle).is_none());
}

fn fixture(root: &std::path::Path) -> (crate::store::MailStore, Account, String) {
    let store = crate::store::MailStore::new(root);
    let data = b"Message-ID: <one@example.com>\r\nSubject: hi\r\n\r\nbody";
    let path = store.store_message("inbox", "inbox-42", data).unwrap();
    let handle = handle_from_bytes(data);
    let mut catalog = Catalog::open(root).unwrap();
    let mut entry = entry(&handle, path.to_string_lossy().to_string());
    entry.remote = Some(remote(&handle));
    catalog.upsert(&entry).unwrap();
    (store, account(), handle)
}

fn entry(handle: &str, raw_path: String) -> crate::catalog::CatalogEntry {
    crate::catalog::CatalogEntry {
        handle: handle.into(),
        raw_path,
        fingerprint: "fingerprint".into(),
        account: "acct".into(),
        folder: "INBOX".into(),
        maildir_subdir: "new".into(),
        date: "2026-05-02 00:00".into(),
        from: "a@example.com".into(),
        to: String::new(),
        cc: String::new(),
        bcc: String::new(),
        subject: "hi".into(),
        rfc_message_id: "one@example.com".into(),
        remote: None,
        is_duplicate: false,
    }
}

fn remote(_handle: &str) -> RemoteIdentity {
    RemoteIdentity {
        account: "acct".into(),
        provider: "protonmail".into(),
        remote_mailbox: "INBOX".into(),
        local_folder: "inbox".into(),
        uid: 42,
        uidvalidity: 7,
        rfc_message_id: "one@example.com".into(),
        size: 54,
        content_fingerprint: "fingerprint".into(),
    }
}

fn capabilities(move_supported: bool, uidplus: bool) -> MutationCapabilities {
    MutationCapabilities {
        move_supported,
        uidplus,
    }
}

fn account() -> Account {
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
        label_roots: None,
        provider: Provider::Protonmail,
        oauth_authorization_url: None,
        oauth_token_url: None,
        oauth_scope: None,
        reject_invalid_certs: None,
    }
}

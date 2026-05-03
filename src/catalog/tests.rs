use super::*;
use std::fs;
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

#[test]
fn handle_is_stable_for_same_content() {
    let data = b"Subject: test\r\nFrom: a@b\r\nTo: c@d\r\n\r\nhello";
    let h1 = handle_from_bytes(data);
    let h2 = handle_from_bytes(data);
    assert_eq!(h1, h2);
}

#[test]
fn handle_differs_for_different_content() {
    let data1 = b"Subject: test1\r\n\r\na";
    let data2 = b"Subject: test2\r\n\r\na";
    assert_ne!(handle_from_bytes(data1), handle_from_bytes(data2));
}

#[test]
fn catalog_opens_and_closes() {
    let tmp = tempfile::tempdir().unwrap();
    let store = crate::store::MailStore::new(tmp.path());
    let catalog = Catalog::open(store.root()).unwrap();
    assert_eq!(catalog.count_messages("test").unwrap(), 0);
}

#[cfg(unix)]
#[test]
fn catalog_uses_private_permissions() {
    let tmp = tempfile::tempdir().unwrap();
    let store = crate::store::MailStore::new(tmp.path());
    let mut catalog = Catalog::open(store.root()).unwrap();
    let entry = entry("abc123", "/test.msg", "acct", "INBOX", "new");

    catalog.upsert(&entry).unwrap();

    assert_eq!(mode(&store.root().join(".vivarium")), 0o700);
    assert_eq!(mode(&store.root().join(".vivarium/catalog.sqlite")), 0o600);
}

#[test]
fn catalog_upsert_and_list() {
    let tmp = tempfile::tempdir().unwrap();
    let store = crate::store::MailStore::new(tmp.path());
    let mut catalog = Catalog::open(store.root()).unwrap();
    let entry = entry("abc123", "/test.msg", "acct", "INBOX", "new");

    catalog.upsert(&entry).unwrap();
    assert_eq!(catalog.count_messages("acct").unwrap(), 1);

    let entries = catalog.list_messages("acct").unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].handle, "abc123");
    assert!(!entries[0].is_duplicate);
}

#[test]
fn catalog_rebuild_stable_handles() {
    let tmp = tempfile::tempdir().unwrap();
    let mail_root = tmp.path().join("mail");
    fs::create_dir_all(&mail_root).unwrap();

    let msg_data =
        b"Subject: stable\r\nFrom: a@b\r\nTo: c@d\r\nMessage-ID: <test@example.com>\r\n\r\nbody";
    fs::write(mail_root.join("inbox-1.eml"), msg_data).unwrap();

    let handle1 = handle_from_bytes(msg_data);
    let fp1 = fingerprint(msg_data);
    let mut catalog = Catalog::open(&mail_root).unwrap();
    let mut entry = entry(&handle1, "inbox-1.eml", "test", "INBOX", "new");
    entry.fingerprint = fp1.clone();
    entry.subject = "stable".into();
    entry.rfc_message_id = "test@example.com".into();
    catalog.upsert(&entry).unwrap();

    let catalog2 = Catalog::open(&mail_root).unwrap();
    let handles = catalog2.list_messages("test").unwrap();
    assert_eq!(handles.len(), 1);
    assert_eq!(handles[0].handle, handle1);
    assert_eq!(handles[0].fingerprint, fp1);
}

#[test]
fn catalog_loads_entries_without_remote_identity_field() {
    let tmp = tempfile::tempdir().unwrap();
    let catalog_dir = tmp.path().join(".vivarium");
    fs::create_dir_all(&catalog_dir).unwrap();
    fs::write(
        catalog_dir.join("catalog.json"),
        r#"[{
          "handle": "abc123",
          "raw_path": "/tmp/inbox-1.eml",
          "fingerprint": "f1",
          "account": "acct",
          "folder": "INBOX",
          "maildir_subdir": "new",
          "date": "2025-01-01 00:00",
          "from": "a@b",
          "to": "c@d",
          "cc": "",
          "bcc": "",
          "subject": "hi",
          "rfc_message_id": "one@example.com",
          "is_duplicate": false
        }]"#,
    )
    .unwrap();

    let catalog = Catalog::open(tmp.path()).unwrap();
    let entries = catalog.list_messages("acct").unwrap();

    assert_eq!(entries.len(), 1);
    assert!(entries[0].remote.is_none());
    assert!(catalog_dir.join("catalog.sqlite").exists());
}

#[test]
fn catalog_duplicate_same_handle_replaces() {
    let tmp = tempfile::tempdir().unwrap();
    let mail_root = tmp.path().join("mail");
    fs::create_dir_all(&mail_root).unwrap();

    let msg_data = b"Subject: dup\r\nFrom: a@b\r\nTo: c@d\r\n\r\ndup content";
    let handle = handle_from_bytes(msg_data);
    let mut catalog = Catalog::open(&mail_root).unwrap();
    let mut first = entry(&handle, "INBOX/inbox.eml", "test", "INBOX", "new");
    first.fingerprint = fingerprint(msg_data);
    catalog.upsert(&first).unwrap();

    let mut second = entry(&handle, "Archive/cur/dup.eml", "test", "Archive", "cur");
    second.fingerprint = fingerprint(msg_data);
    catalog.upsert(&second).unwrap();

    let entries = catalog.list_messages("test").unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].raw_path, "Archive/cur/dup.eml");
}

#[test]
fn attach_remote_identity_matches_by_rfc_message_id() {
    let tmp = tempfile::tempdir().unwrap();
    let mut catalog = Catalog::open(tmp.path()).unwrap();
    let mut existing = entry(
        "abc123",
        "/mail/INBOX/new/inbox-42.eml",
        "acct",
        "INBOX",
        "new",
    );
    existing.rfc_message_id = "one@example.com".into();
    existing.fingerprint = "fingerprint".into();
    catalog.upsert(&existing).unwrap();

    let result = catalog
        .attach_remote_identities(&[candidate("acct", "INBOX", "inbox", 42, Some(9))])
        .unwrap();
    let remote = catalog.remote_reference("acct", "abc123").unwrap();

    assert_eq!(result.matched, 1);
    assert_eq!(remote.account, "acct");
    assert_eq!(remote.provider, "protonmail");
    assert_eq!(remote.remote_mailbox, "INBOX");
    assert_eq!(remote.local_folder, "inbox");
    assert_eq!(remote.uid, 42);
    assert_eq!(remote.uidvalidity, 9);
    assert_eq!(remote.content_fingerprint, "fingerprint");
}

#[test]
fn attach_remote_identity_falls_back_to_local_uid_filename() {
    let tmp = tempfile::tempdir().unwrap();
    let mut catalog = Catalog::open(tmp.path()).unwrap();
    let existing = entry(
        "abc123",
        "/mail/INBOX/new/inbox-7.eml",
        "acct",
        "INBOX",
        "new",
    );
    catalog.upsert(&existing).unwrap();

    let result = catalog
        .attach_remote_identities(&[RemoteIdentityCandidate {
            rfc_message_id: None,
            ..candidate("acct", "INBOX", "inbox", 7, Some(11))
        }])
        .unwrap();

    assert_eq!(result.matched, 1);
    assert_eq!(
        catalog
            .remote_reference("acct", "abc123")
            .unwrap()
            .uidvalidity,
        11
    );
}

#[test]
fn remote_reference_status_reports_missing_and_stale_states() {
    let tmp = tempfile::tempdir().unwrap();
    let mut catalog = Catalog::open(tmp.path()).unwrap();
    let existing = entry(
        "abc123",
        "/mail/INBOX/new/inbox-42.eml",
        "acct",
        "INBOX",
        "new",
    );
    catalog.upsert(&existing).unwrap();

    assert!(matches!(
        catalog.remote_reference_status("acct", "missing", None),
        RemoteReferenceStatus::MissingHandle { .. }
    ));
    assert!(matches!(
        catalog.remote_reference_status("acct", "abc123", None),
        RemoteReferenceStatus::MissingRemoteIdentity { .. }
    ));

    catalog
        .attach_remote_identities(&[candidate("acct", "INBOX", "inbox", 42, Some(9))])
        .unwrap();

    assert!(matches!(
        catalog.remote_reference_status("acct", "abc123", Some(10)),
        RemoteReferenceStatus::StaleUidValidity {
            stored_uidvalidity: 9,
            current_uidvalidity: 10,
            ..
        }
    ));
}

#[test]
fn attach_remote_identity_reports_ambiguous_duplicate_matches() {
    let tmp = tempfile::tempdir().unwrap();
    let mut catalog = Catalog::open(tmp.path()).unwrap();
    let mut first = entry(
        "first",
        "/mail/INBOX/new/inbox-1.eml",
        "acct",
        "INBOX",
        "new",
    );
    first.rfc_message_id = "same@example.com".into();
    let mut second = entry(
        "second",
        "/mail/INBOX/new/inbox-2.eml",
        "acct",
        "INBOX",
        "new",
    );
    second.rfc_message_id = "same@example.com".into();
    catalog.upsert(&first).unwrap();
    catalog.upsert(&second).unwrap();

    let result = catalog
        .attach_remote_identities(&[RemoteIdentityCandidate {
            rfc_message_id: Some("same@example.com".into()),
            ..candidate("acct", "INBOX", "inbox", 1, Some(9))
        }])
        .unwrap();

    assert_eq!(result.matched, 0);
    assert_eq!(result.ambiguous, 1);
}

#[test]
fn attach_remote_identity_skips_missing_uidvalidity() {
    let tmp = tempfile::tempdir().unwrap();
    let mut catalog = Catalog::open(tmp.path()).unwrap();
    let existing = entry(
        "abc123",
        "/mail/INBOX/new/inbox-42.eml",
        "acct",
        "INBOX",
        "new",
    );
    catalog.upsert(&existing).unwrap();

    let result = catalog
        .attach_remote_identities(&[candidate("acct", "INBOX", "inbox", 42, None)])
        .unwrap();

    assert_eq!(result.matched, 0);
    assert_eq!(result.missing_uidvalidity, 1);
}

#[test]
fn remove_entry_does_not_remove_same_handle_for_other_account() {
    let tmp = tempfile::tempdir().unwrap();
    let mut catalog = Catalog::open(tmp.path()).unwrap();
    let existing = entry(
        "abc123",
        "/mail/INBOX/new/inbox-42.eml",
        "acct",
        "INBOX",
        "new",
    );
    catalog.upsert(&existing).unwrap();

    let err = catalog.remove_entry("other", "abc123").unwrap_err();
    let kept = catalog.entry("acct", "abc123").unwrap();

    assert!(err.to_string().contains("message not found"));
    assert_eq!(kept.account, "acct");
}

#[test]
fn update_maildir_catalogs_only_uncataloged_paths() {
    let tmp = tempfile::tempdir().unwrap();
    let store = crate::store::MailStore::new(tmp.path());
    store
        .store_message(
            "inbox",
            "inbox-1",
            b"Message-ID: <one@example.com>\r\nFrom: a@b\r\nTo: c@d\r\nSubject: one\r\n\r\nbody one",
        )
        .unwrap();

    let first = update_maildir(tmp.path(), "acct", &store).unwrap();
    let second = update_maildir(tmp.path(), "acct", &store).unwrap();

    assert_eq!(first.scanned, 1);
    assert_eq!(first.cataloged, 1);
    assert_eq!(first.skipped, 0);
    assert_eq!(second.scanned, 1);
    assert_eq!(second.cataloged, 0);
    assert_eq!(second.skipped, 1);
}

fn entry(
    handle: &str,
    raw_path: &str,
    account: &str,
    folder: &str,
    maildir_subdir: &str,
) -> CatalogEntry {
    CatalogEntry {
        handle: handle.into(),
        raw_path: raw_path.into(),
        fingerprint: "f1".into(),
        account: account.into(),
        folder: folder.into(),
        maildir_subdir: maildir_subdir.into(),
        date: "2025-01-01 00:00".into(),
        from: "a@b".into(),
        to: "c@d".into(),
        cc: String::new(),
        bcc: String::new(),
        subject: "hi".into(),
        rfc_message_id: String::new(),
        remote: None,
        is_duplicate: false,
    }
}

fn candidate(
    account: &str,
    remote_mailbox: &str,
    local_folder: &str,
    uid: u32,
    uidvalidity: Option<u32>,
) -> RemoteIdentityCandidate {
    RemoteIdentityCandidate {
        account: account.into(),
        provider: "protonmail".into(),
        remote_mailbox: remote_mailbox.into(),
        local_folder: local_folder.into(),
        uid,
        uidvalidity,
        rfc_message_id: Some("one@example.com".into()),
        size: 123,
    }
}

#[cfg(unix)]
fn mode(path: &std::path::Path) -> u32 {
    fs::metadata(path).unwrap().permissions().mode() & 0o777
}

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
    assert_eq!(mode(&store.root().join(".vivarium/catalog.json")), 0o600);
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
        is_duplicate: false,
    }
}

#[cfg(unix)]
fn mode(path: &std::path::Path) -> u32 {
    fs::metadata(path).unwrap().permissions().mode() & 0o777
}

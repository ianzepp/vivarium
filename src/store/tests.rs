use std::collections::HashMap;
use std::fs;
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;

use super::*;

#[test]
fn ensure_folders_creates_maildirs() {
    let tmp = tempfile::tempdir().unwrap();
    let store = MailStore::new(tmp.path());
    store.ensure_folders().unwrap();
    for folder in FOLDERS {
        assert!(tmp.path().join(folder).join("new").is_dir());
        assert!(tmp.path().join(folder).join("cur").is_dir());
        assert!(tmp.path().join(folder).join("tmp").is_dir());
    }
}

#[cfg(unix)]
#[test]
fn ensure_folders_creates_private_maildirs() {
    let tmp = tempfile::tempdir().unwrap();
    let store = MailStore::new(tmp.path());
    store.ensure_folders().unwrap();

    assert_eq!(mode(tmp.path()), 0o700);
    assert_eq!(mode(&tmp.path().join("INBOX")), 0o700);
    assert_eq!(mode(&tmp.path().join("INBOX/new")), 0o700);
}

#[test]
fn store_message_writes_via_maildir() {
    let tmp = tempfile::tempdir().unwrap();
    let store = MailStore::new(tmp.path());

    let path = store
        .store_message("inbox", "inbox-1", b"Subject: hello\r\n\r\nbody")
        .unwrap();

    assert_eq!(path, tmp.path().join("INBOX/new/inbox-1.eml"));
    assert_eq!(
        store.read_message("inbox-1").unwrap(),
        b"Subject: hello\r\n\r\nbody"
    );
}

#[test]
fn list_messages_rejects_unknown_folder() {
    let tmp = tempfile::tempdir().unwrap();
    let store = MailStore::new(tmp.path());

    let err = store.list_messages("bogus").unwrap_err();

    assert!(err.to_string().contains("invalid folder"));
}

#[cfg(unix)]
#[test]
fn store_message_writes_private_file() {
    let tmp = tempfile::tempdir().unwrap();
    let store = MailStore::new(tmp.path());

    let path = store
        .store_message("inbox", "inbox-1", b"Subject: hello\r\n\r\nbody")
        .unwrap();

    assert_eq!(mode(&path), 0o600);
}

#[test]
fn message_ids_ignore_maildir_flags() {
    let path = PathBuf::from("INBOX/cur/inbox-1.eml:2,S");
    assert_eq!(message_id_from_path(&path).unwrap(), "inbox-1");
}

#[test]
fn move_message_preserves_source_subdir() {
    let tmp = tempfile::tempdir().unwrap();
    let store = MailStore::new(tmp.path());
    let src = store
        .store_message("inbox", "inbox-1", b"Subject: hello\r\n\r\nbody")
        .unwrap();

    let dst = store.move_message("inbox-1", "inbox", "archive").unwrap();

    assert_eq!(src.parent().unwrap().file_name().unwrap(), "new");
    assert_eq!(dst, tmp.path().join("Archive/new/inbox-1.eml"));
    assert!(dst.exists());
}

#[test]
fn move_message_rejects_tmp_source() {
    let tmp = tempfile::tempdir().unwrap();
    let store = MailStore::new(tmp.path());
    store.ensure_folders().unwrap();
    fs::write(
        tmp.path().join("INBOX/tmp/inbox-1.eml"),
        b"Subject: hello\r\n\r\nbody",
    )
    .unwrap();

    let err = store
        .move_message("inbox-1", "inbox", "archive")
        .unwrap_err();
    assert!(err.to_string().contains("unexpected maildir subdirectory"));
}

#[test]
fn rfc_index_builds_from_local_files() {
    let tmp = tempfile::tempdir().unwrap();
    let store = MailStore::new(tmp.path());
    let data1 = b"Message-ID: <ABC@example.COM>\r\nSubject: hello\r\n\r\nbody";
    let data2 = b"Message-ID: <DEF@example.COM>\r\nSubject: world\r\n\r\nmore";
    store.store_message("inbox", "inbox-7", data1).unwrap();
    store.store_message("inbox", "inbox-8", data2).unwrap();

    let index = store.build_rfc_index("inbox").unwrap();
    assert_eq!(index.get("abc@example.com"), Some(&(7, data1.len() as u64)));
    assert_eq!(index.get("def@example.com"), Some(&(8, data2.len() as u64)));
    assert!(!index.contains_key("nonexistent@example.com"));
}

#[test]
fn rfc_index_lookup_matches_correct_size() {
    let tmp = tempfile::tempdir().unwrap();
    let store = MailStore::new(tmp.path());
    let index = HashMap::from([("abc@example.com".to_string(), (42, 99u64))]);

    assert!(store.rfc_index_lookup(&index, "abc@example.com", 99));
    assert!(!store.rfc_index_lookup(&index, "abc@example.com", 100));
    assert!(!store.rfc_index_lookup(&index, "other@example.com", 99));
}

#[test]
fn rfc_index_skips_files_without_message_id() {
    let tmp = tempfile::tempdir().unwrap();
    let store = MailStore::new(tmp.path());
    let data_with_id = b"Message-ID: <test@example.com>\r\nSubject: hi\r\n\r\n";
    store
        .store_message("inbox", "inbox-1", data_with_id)
        .unwrap();
    let data_without_id = b"Subject: no id\r\n\r\n";
    store
        .store_message("inbox", "inbox-2", data_without_id)
        .unwrap();

    let index = store.build_rfc_index("inbox").unwrap();
    assert_eq!(index.len(), 1);
    assert!(index.contains_key("test@example.com"));
}

#[cfg(unix)]
fn mode(path: &std::path::Path) -> u32 {
    fs::metadata(path).unwrap().permissions().mode() & 0o777
}

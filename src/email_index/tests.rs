use std::fs;
use std::path::Path;

use super::EmailIndex;
use crate::catalog::{Catalog, CatalogEntry};
use crate::store::MailStore;

#[test]
fn rebuild_indexes_messages_and_links_without_text() {
    let tmp = tempfile::tempdir().unwrap();
    let store = MailStore::new(tmp.path());
    let path = store
        .store_message(
            "inbox",
            "inbox-1",
            b"Message-ID: <Root@Example.COM>\r\nReferences: <parent@example.com>\r\nSubject: root\r\n\r\nsecret body",
        )
        .unwrap();
    catalog(tmp.path(), "acct", "cat-1", &path, "inbox", "new");

    let stats = EmailIndex::rebuild(tmp.path(), "acct").unwrap();
    let index = EmailIndex::open(tmp.path()).unwrap();
    let message = index.message("acct", "inbox-1").unwrap().unwrap();
    let ids = index.related_ids("acct", "inbox-1").unwrap();

    assert_eq!(stats.scanned, 1);
    assert_eq!(message.rfc_message_id.as_deref(), Some("root@example.com"));
    assert!(ids.contains("root@example.com"));
    assert!(ids.contains("parent@example.com"));

    let db = fs::read(tmp.path().join(".vivarium/index.sqlite")).unwrap();
    assert!(!String::from_utf8_lossy(&db).contains("secret body"));
}

#[test]
fn thread_messages_finds_reply_by_reference() {
    let tmp = tempfile::tempdir().unwrap();
    let store = MailStore::new(tmp.path());
    let root = store
        .store_message(
            "inbox",
            "inbox-1",
            b"Message-ID: <root@example.com>\r\nDate: Sat, 2 May 2026 12:00:00 +0000\r\nSubject: root\r\n\r\nroot body",
        )
        .unwrap();
    let reply = store
        .store_message_in(
            "sent",
            "cur",
            "sent-2",
            b"Message-ID: <reply@example.com>\r\nIn-Reply-To: <root@example.com>\r\nReferences: <root@example.com>\r\nDate: Sat, 2 May 2026 12:01:00 +0000\r\nSubject: Re: root\r\n\r\nreply body",
        )
        .unwrap();
    catalog(tmp.path(), "acct", "cat-root", &root, "INBOX", "new");
    catalog(tmp.path(), "acct", "cat-reply", &reply, "Sent", "cur");

    EmailIndex::rebuild(tmp.path(), "acct").unwrap();
    let index = EmailIndex::open(tmp.path()).unwrap();
    let messages = index.thread_messages("acct", "inbox-1", 50).unwrap();

    assert_eq!(
        messages
            .iter()
            .map(|m| m.handle.as_str())
            .collect::<Vec<_>>(),
        vec!["inbox-1", "sent-2"]
    );
}

fn catalog(mail_root: &Path, account: &str, handle: &str, path: &Path, folder: &str, subdir: &str) {
    let data = fs::read(path).unwrap();
    let mut catalog = Catalog::open(mail_root).unwrap();
    catalog
        .upsert(&CatalogEntry {
            handle: handle.to_string(),
            raw_path: path.to_string_lossy().to_string(),
            fingerprint: crate::catalog::fingerprint(&data),
            account: account.to_string(),
            folder: folder.to_string(),
            maildir_subdir: subdir.to_string(),
            date: "2026-05-02 12:00".to_string(),
            from: String::new(),
            to: String::new(),
            cc: String::new(),
            bcc: String::new(),
            subject: String::new(),
            rfc_message_id: crate::message::message_id_from_bytes(&data).unwrap_or_default(),
            remote: None,
            is_duplicate: false,
        })
        .unwrap();
}

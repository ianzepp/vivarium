use crate::storage::{MessageIngestRequest, Storage};

use super::*;

#[test]
fn json_message_includes_citation() {
    let tmp = tempfile::tempdir().unwrap();
    let store = MailStore::new(tmp.path());
    ingest_storage_message(
        tmp.path(),
        "msg_json",
        b"Message-ID: <a@example.com>\r\nFrom: A <a@example.com>\r\nTo: B <b@example.com>\r\nSubject: hello\r\n\r\nbody",
    );

    let json = json_message(&store, "acct", "msg_json").unwrap();

    assert_eq!(json["handle"], "json");
    assert_eq!(json["citation"]["account"], "acct");
    assert_eq!(json["citation"]["local_role"], "inbox");
    assert_eq!(json["citation"]["message_id"], "msg_json");
    assert!(json["citation"]["content_id"].as_str().is_some());
}

#[test]
fn export_text_uses_extracted_body() {
    let tmp = tempfile::tempdir().unwrap();
    let store = MailStore::new(tmp.path());
    ingest_storage_message(
        tmp.path(),
        "msg_text",
        b"From: a@example.com\r\nTo: b@example.com\r\nSubject: hello\r\n\r\nbody text",
    );

    let data = store.read_message("msg_text").unwrap();
    let extracted = extract::extract_text(&data).unwrap();

    assert_eq!(extracted.body_text, "body text");
}

fn ingest_storage_message(root: &std::path::Path, message_id: &str, data: &[u8]) {
    Storage::open(root)
        .unwrap()
        .ingest_message(
            &MessageIngestRequest {
                account: "acct".into(),
                local_role: "inbox".into(),
                read_state: false,
                starred: false,
                message_id_hint: Some(message_id.into()),
                seed_hint: message_id.into(),
                remote: None,
            },
            data,
        )
        .unwrap();
}

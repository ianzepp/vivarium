use super::*;
use crate::proton_api::{ProtonAddress, ProtonMessage};

#[test]
fn raw_message_cache_round_trips_full_message() {
    let tmp = tempfile::tempdir().unwrap();
    let cache = ProtonRawMessageCache::new(tmp.path());
    let message = full_message("proton-id");

    cache.store(&message).unwrap();

    assert_eq!(cache.load("proton-id").unwrap(), Some(message));
}

#[test]
fn raw_message_cache_missing_file_returns_none() {
    let tmp = tempfile::tempdir().unwrap();
    let cache = ProtonRawMessageCache::new(tmp.path());

    assert_eq!(cache.load("missing").unwrap(), None);
}

#[cfg(unix)]
#[test]
fn raw_message_cache_uses_private_permissions() {
    let tmp = tempfile::tempdir().unwrap();
    let cache = ProtonRawMessageCache::new(tmp.path());
    cache.store(&full_message("private-id")).unwrap();

    let path = cache.path_for("private-id");
    let mode = fs::metadata(path).unwrap().permissions().mode() & 0o777;

    assert_eq!(mode, 0o600);
}

fn full_message(id: &str) -> ProtonFullMessage {
    ProtonFullMessage {
        metadata: ProtonMessage {
            id: id.into(),
            conversation_id: "conversation-id".into(),
            external_id: "external@example.com".into(),
            subject: "hello".into(),
            time: 1_778_205_000,
            size: 123,
            flags: 4,
            unread: 0,
            num_attachments: 0,
            sender: ProtonAddress {
                name: "Sender".into(),
                address: "sender@example.com".into(),
            },
            to: Vec::new(),
            cc: Vec::new(),
            bcc: Vec::new(),
            label_ids: vec!["0".into()],
        },
        header: "Subject: hello\r\n\r\n".into(),
        body: "encrypted body".into(),
        mime_type: "text/plain".into(),
    }
}

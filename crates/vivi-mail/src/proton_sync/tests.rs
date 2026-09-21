use super::*;
use crate::config::StorageMode;
use crate::proton_api::ProtonAddress;

#[test]
fn direct_sync_treats_semantic_as_body_capable_storage() {
    let mut account = test_account(StorageMode::Semantic);

    assert!(direct_sync_storage_supported(&account));
    assert!(account.stores_full_bodies());
    assert!(account.allows_semantic_indexing());

    account.storage_mode = Some(StorageMode::Proxy);

    assert!(!direct_sync_storage_supported(&account));
}

#[test]
fn maps_system_labels_to_local_roles() {
    assert_eq!(local_role(&["0".into()]), "inbox");
    assert_eq!(local_role(&["2".into()]), "sent");
    assert_eq!(local_role(&["1".into()]), "drafts");
    assert_eq!(local_role(&["3".into()]), "trash");
    assert_eq!(local_role(&["5".into()]), "archive");
}

#[test]
fn header_bytes_redact_body_and_include_metadata() {
    let message = test_message();
    let header = String::from_utf8(header_bytes(&message)).unwrap();

    assert!(header.contains("Subject: hello"));
    assert!(header.contains("Message-ID: <external@example.com>"));
    assert!(header.contains("X-Proton-Message-ID: proton-id"));
    assert!(header.contains("X-Proton-Num-Attachments: 2"));
    assert!(header.ends_with("\r\n\r\n"));
}

#[test]
fn body_bytes_normalize_header_and_append_cleartext() {
    let message = ProtonFullMessage {
        metadata: test_message(),
        header: "Subject: hello\nContent-Type: text/plain\n\n".into(),
        body: String::new(),
        mime_type: "text/plain".into(),
    };
    let bytes = body_bytes(&message, b"clear body");

    assert_eq!(
        String::from_utf8(bytes).unwrap(),
        "Subject: hello\r\nContent-Type: text/plain\r\n\r\nclear body"
    );
}

#[test]
fn decryption_failure_bytes_record_local_marker() {
    let bytes = decryption_failure_bytes(&test_message());
    let message = String::from_utf8(bytes).unwrap();

    assert!(message.contains("X-Vivarium-Proton-Decryption-Error: true\r\n"));
    assert!(message.ends_with("\r\n\r\n"));
}

fn test_message() -> ProtonMessage {
    ProtonMessage {
        id: "proton-id".into(),
        conversation_id: "conversation-id".into(),
        external_id: "external@example.com".into(),
        subject: "hello".into(),
        time: 1_778_205_000,
        size: 123,
        flags: 4,
        unread: 0,
        num_attachments: 2,
        sender: ProtonAddress {
            name: "Sender".into(),
            address: "sender@example.com".into(),
        },
        to: vec![ProtonAddress {
            name: String::new(),
            address: "to@example.com".into(),
        }],
        cc: Vec::new(),
        bcc: Vec::new(),
        label_ids: vec!["0".into(), "5".into()],
    }
}

fn test_account(storage_mode: StorageMode) -> crate::config::Account {
    crate::config::Account {
        name: "direct".into(),
        email: "agent@example.com".into(),
        imap_host: String::new(),
        imap_port: None,
        imap_security: None,
        smtp_host: String::new(),
        smtp_port: None,
        smtp_security: None,
        username: "agent@example.com".into(),
        auth: crate::config::Auth::Password,
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
        storage_mode: Some(storage_mode),
        provider: crate::config::Provider::ProtonApi,
        oauth_authorization_url: None,
        oauth_token_url: None,
        oauth_scope: None,
        reject_invalid_certs: None,
        policy: crate::config::MutationPolicy::FullWrite,
    }
}

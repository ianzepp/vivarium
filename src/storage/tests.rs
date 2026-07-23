use super::*;

#[test]
fn import_dedupes_blobs_but_keeps_distinct_message_rows() {
    let tmp = tempfile::tempdir().unwrap();
    let raw = message_bytes("dup@example.com", "same body");
    let first = write_catalog_file(tmp.path(), "inbox-1.eml", &raw);
    let second = write_catalog_file(tmp.path(), "archive-2.eml", &raw);

    let entries = vec![
        catalog_entry("acct", "one", &first, "INBOX", Some(remote("INBOX", 7))),
        catalog_entry(
            "acct",
            "two",
            &second,
            "Archive",
            Some(remote("Archive", 8)),
        ),
    ];

    let result = import_catalog_entries(tmp.path(), &entries).unwrap();
    let storage = Storage::open(tmp.path()).unwrap();

    assert_eq!(result.imported_messages, 2);
    assert_eq!(result.imported_blobs, 1);
    assert_eq!(storage.blob_count().unwrap(), 1);
    assert_eq!(storage.message_count().unwrap(), 2);
    assert_eq!(storage.remote_binding_count().unwrap(), 2);
}

#[test]
fn import_persists_blob_and_metadata() {
    let tmp = tempfile::tempdir().unwrap();
    let raw = b"Message-ID: <meta@example.com>\r\nFrom: Agent <agent@example.com>\r\nTo: User <user@example.com>\r\nSubject: hello\r\n\r\nbody";
    let path = write_catalog_file(tmp.path(), "inbox-1.eml", raw);
    let entries = vec![catalog_entry(
        "acct",
        "one",
        &path,
        "INBOX",
        Some(remote("INBOX", 7)),
    )];

    import_catalog_entries(tmp.path(), &entries).unwrap();
    let storage = Storage::open(tmp.path()).unwrap();
    let data = storage.read_blob(&resulting_content_id(raw)).unwrap();

    assert_eq!(data, raw);
    assert_eq!(storage.blob_count().unwrap(), 1);
    assert_eq!(storage.message_count().unwrap(), 1);
}

#[test]
fn fallback_message_ids_are_stable_for_unbound_entries() {
    let tmp = tempfile::tempdir().unwrap();
    let raw = message_bytes("local@example.com", "body");
    let path = write_catalog_file(tmp.path(), "draft-1.eml", &raw);
    let entry = catalog_entry("acct", "draft-handle", &path, "Drafts", None);

    let mut storage = Storage::open(tmp.path()).unwrap();
    let first = storage
        .ingest_message(&request_from_catalog_entry(&entry), &raw)
        .unwrap();
    let second = storage
        .ingest_message(&request_from_catalog_entry(&entry), &raw)
        .unwrap();

    assert_eq!(first.message_id, second.message_id);
    assert_eq!(storage.message_count().unwrap(), 1);
}

#[test]
fn direct_ingest_api_supports_clean_break_sync_target() {
    let tmp = tempfile::tempdir().unwrap();
    let raw = message_bytes("direct@example.com", "body");
    let request = MessageIngestRequest {
        account: "acct".into(),
        local_role: "inbox".into(),
        read_state: false,
        starred: false,
        message_id_hint: None,
        seed_hint: "uid:99".into(),
        remote: Some(RemoteBindingInput {
            account: "acct".into(),
            provider: "protonmail".into(),
            remote_mailbox: "INBOX".into(),
            remote_uid: 99,
            remote_uidvalidity: 42,
        }),
    };

    let mut storage = Storage::open(tmp.path()).unwrap();
    let stored = storage.ingest_message(&request, &raw).unwrap();

    assert!(stored.message_id.starts_with("msg_"));
    assert_eq!(storage.blob_count().unwrap(), 1);
    assert_eq!(storage.message_count().unwrap(), 1);
    assert_eq!(storage.remote_binding_count().unwrap(), 1);
}

#[test]
fn short_handles_resolve_uniquely_for_storage_native_ids() {
    let tmp = tempfile::tempdir().unwrap();
    let mut storage = Storage::open(tmp.path()).unwrap();

    let first = storage
        .ingest_message(
            &MessageIngestRequest {
                account: "acct".into(),
                local_role: "inbox".into(),
                read_state: false,
                starred: false,
                message_id_hint: None,
                seed_hint: "remote_uid:1".into(),
                remote: Some(RemoteBindingInput {
                    account: "acct".into(),
                    provider: "protonmail".into(),
                    remote_mailbox: "INBOX".into(),
                    remote_uid: 1,
                    remote_uidvalidity: 42,
                }),
            },
            &message_bytes("one@example.com", "first"),
        )
        .unwrap();
    let second = storage
        .ingest_message(
            &MessageIngestRequest {
                account: "acct".into(),
                local_role: "inbox".into(),
                read_state: false,
                starred: false,
                message_id_hint: None,
                seed_hint: "remote_uid:2".into(),
                remote: Some(RemoteBindingInput {
                    account: "acct".into(),
                    provider: "protonmail".into(),
                    remote_mailbox: "INBOX".into(),
                    remote_uid: 2,
                    remote_uidvalidity: 42,
                }),
            },
            &message_bytes("two@example.com", "second"),
        )
        .unwrap();

    let first_handle = storage.display_handle(&first.message_id).unwrap();
    let second_handle = storage.display_handle(&second.message_id).unwrap();

    assert_ne!(first_handle, second_handle);
    assert!(first_handle.len() >= 7);
    assert_eq!(
        storage.resolve_message_token(&first_handle).unwrap(),
        first.message_id
    );
    assert_eq!(
        storage.resolve_message_token(&second_handle).unwrap(),
        second.message_id
    );
}

#[test]
fn content_id_prefix_can_resolve_message() {
    let tmp = tempfile::tempdir().unwrap();
    let raw = message_bytes("content@example.com", "body");
    let mut storage = Storage::open(tmp.path()).unwrap();
    let stored = storage
        .ingest_message(
            &MessageIngestRequest {
                account: "acct".into(),
                local_role: "inbox".into(),
                read_state: false,
                starred: false,
                message_id_hint: None,
                seed_hint: "remote_uid:3".into(),
                remote: Some(RemoteBindingInput {
                    account: "acct".into(),
                    provider: "protonmail".into(),
                    remote_mailbox: "INBOX".into(),
                    remote_uid: 3,
                    remote_uidvalidity: 42,
                }),
            },
            &raw,
        )
        .unwrap();

    let prefix = &stored.content_id[..12];
    assert_eq!(
        storage.resolve_message_token(prefix).unwrap(),
        stored.message_id
    );
}

#[test]
fn local_size_fallback_uses_remote_uid_shape_for_storage_rows() {
    let tmp = tempfile::tempdir().unwrap();
    let raw = message_bytes("size@example.com", "body");
    let mut storage = Storage::open(tmp.path()).unwrap();
    storage
        .ingest_message(
            &MessageIngestRequest {
                account: "acct".into(),
                local_role: "inbox".into(),
                read_state: false,
                starred: false,
                message_id_hint: None,
                seed_hint: "remote_uid:7".into(),
                remote: Some(RemoteBindingInput {
                    account: "acct".into(),
                    provider: "protonmail".into(),
                    remote_mailbox: "INBOX".into(),
                    remote_uid: 7,
                    remote_uidvalidity: 42,
                }),
            },
            &raw,
        )
        .unwrap();

    let sizes = storage.local_sizes_by_role("inbox").unwrap();
    assert_eq!(sizes.get("inbox-7"), Some(&(raw.len() as u64)));
}

fn message_bytes(message_id: &str, body: &str) -> Vec<u8> {
    format!(
            "Message-ID: <{message_id}>\r\nFrom: Agent <agent@example.com>\r\nTo: User <user@example.com>\r\nSubject: hi\r\n\r\n{body}"
        )
        .into_bytes()
}

fn resulting_content_id(data: &[u8]) -> String {
    sha256_hex(data)
}

fn write_catalog_file(root: &Path, name: &str, data: &[u8]) -> String {
    let path = root.join(name);
    fs::write(&path, data).unwrap();
    path.to_string_lossy().to_string()
}

fn catalog_entry(
    account: &str,
    handle: &str,
    blob_path: &str,
    folder: &str,
    remote: Option<RemoteIdentity>,
) -> CatalogEntry {
    CatalogEntry {
        handle: handle.into(),
        account: account.into(),
        content_id: sha256_hex(&fs::read(blob_path).unwrap()),
        blob_path: blob_path.into(),
        local_role: local_role(folder),
        read_state: false,
        starred: false,
        date: "2026-05-03T12:00:00Z".into(),
        from: "agent@example.com".into(),
        to: "user@example.com".into(),
        cc: String::new(),
        bcc: String::new(),
        subject: "hi".into(),
        rfc_message_id: "meta@example.com".into(),
        remote,
    }
}

fn remote(mailbox: &str, uid: u32) -> RemoteIdentity {
    RemoteIdentity {
        account: "acct".into(),
        provider: "protonmail".into(),
        remote_mailbox: mailbox.into(),
        local_folder: mailbox.to_ascii_lowercase(),
        uid,
        uidvalidity: 42,
        rfc_message_id: "meta@example.com".into(),
        size: 128,
        content_fingerprint: "unused".into(),
    }
}

#[test]
fn move_message_to_role_rejects_wrong_account() {
    let tmp = tempfile::tempdir().unwrap();
    let raw = message_bytes("move@example.com", "body");
    let mut storage = Storage::open(tmp.path()).unwrap();
    let stored = storage
        .ingest_message(
            &MessageIngestRequest {
                account: "acct".into(),
                local_role: "inbox".into(),
                read_state: false,
                starred: false,
                message_id_hint: None,
                seed_hint: "remote_uid:5".into(),
                remote: Some(RemoteBindingInput {
                    account: "acct".into(),
                    provider: "protonmail".into(),
                    remote_mailbox: "INBOX".into(),
                    remote_uid: 5,
                    remote_uidvalidity: 42,
                }),
            },
            &raw,
        )
        .unwrap();

    let err = storage
        .move_message_to_role("other", &stored.message_id, "archive")
        .unwrap_err();
    assert!(err.to_string().contains("not found"));

    // Original message must be unchanged.
    let sizes = storage.local_sizes_by_role("inbox").unwrap();
    assert!(sizes.contains_key("inbox-5"));
}

#[test]
fn mark_message_deleted_is_idempotent() {
    let tmp = tempfile::tempdir().unwrap();
    let raw = message_bytes("del@example.com", "body");
    let mut storage = Storage::open(tmp.path()).unwrap();
    let stored = storage
        .ingest_message(
            &MessageIngestRequest {
                account: "acct".into(),
                local_role: "inbox".into(),
                read_state: false,
                starred: false,
                message_id_hint: None,
                seed_hint: "remote_uid:6".into(),
                remote: Some(RemoteBindingInput {
                    account: "acct".into(),
                    provider: "protonmail".into(),
                    remote_mailbox: "INBOX".into(),
                    remote_uid: 6,
                    remote_uidvalidity: 42,
                }),
            },
            &raw,
        )
        .unwrap();

    // First delete succeeds.
    assert!(
        storage
            .mark_message_deleted("acct", &stored.message_id)
            .unwrap()
    );

    // Second delete returns false — already soft-deleted.
    assert!(
        !storage
            .mark_message_deleted("acct", &stored.message_id)
            .unwrap()
    );
}

// ---------------------------------------------------------------------------
// Test helpers extracted from production modules
// ---------------------------------------------------------------------------

fn local_role(folder: &str) -> String {
    match folder {
        "INBOX" | "Inbox" | "inbox" => "inbox".into(),
        "Archive" | "archive" => "archive".into(),
        "Trash" | "trash" => "trash".into(),
        "Sent" | "sent" => "sent".into(),
        "Drafts" | "drafts" => "drafts".into(),
        other => other.to_ascii_lowercase(),
    }
}

impl Storage {
    fn blob_count(&self) -> Result<usize, VivariumError> {
        self.conn
            .query_row("SELECT COUNT(*) FROM blobs", [], |row| row.get(0))
            .map_err(|e| VivariumError::Other(format!("failed to count blobs: {e}")))
    }

    fn message_count(&self) -> Result<usize, VivariumError> {
        self.conn
            .query_row("SELECT COUNT(*) FROM messages", [], |row| row.get(0))
            .map_err(|e| VivariumError::Other(format!("failed to count messages: {e}")))
    }

    fn remote_binding_count(&self) -> Result<usize, VivariumError> {
        self.conn
            .query_row("SELECT COUNT(*) FROM remote_bindings", [], |row| row.get(0))
            .map_err(|e| VivariumError::Other(format!("failed to count remote bindings: {e}")))
    }
}

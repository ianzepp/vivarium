use super::*;
use crate::imap::identity::remote_identity_candidates;

#[test]
fn inbox_watch_plan_has_no_sent_or_outbound_folder() {
    let account = test_account();
    let plan = inbox_plan(&account);

    assert_eq!(plan.remote_folder, "INBOX");
    assert_eq!(plan.local_folder, "inbox");
}

fn test_account() -> Account {
    Account {
        name: "agent".into(),
        email: "agent@example.com".into(),
        imap_host: "localhost".into(),
        imap_port: Some(993),
        imap_security: None,
        smtp_host: String::new(),
        smtp_port: None,
        smtp_security: None,
        username: "agent".into(),
        auth: Default::default(),
        password: Some("secret".into()),
        password_cmd: None,
        token_cmd: None,
        oauth_client_id: None,
        oauth_client_secret: None,
        mail_dir: None,
        inbox_folder: None,
        archive_folder: None,
        trash_folder: None,
        sent_folder: Some("Sent".into()),
        drafts_folder: Some("Drafts".into()),
        label_roots: None,
        storage_mode: None,
        provider: Default::default(),
        oauth_authorization_url: None,
        oauth_token_url: None,
        oauth_scope: None,
        reject_invalid_certs: None,
        policy: Default::default(),
    }
}

#[test]
fn find_missing_skips_remote_uid_remap_when_message_id_matches() {
    let tmp = tempfile::tempdir().unwrap();
    let store = MailStore::new(tmp.path());
    ingest_storage_message(
        tmp.path(),
        "inbox",
        "remote_uid:1",
        b"Message-ID: <stable@example.com>\r\nSubject: old\r\n\r\nbody",
    );

    let remote = RemoteMessage {
        uid: 9001,
        uidvalidity: Some(123),
        size: 999,
        rfc_message_id: Some("stable@example.com".to_string()),
        read_state: true,
        starred: false,
    };

    let missing = find_missing(&[remote], &store, "inbox", DedupeScope::LocalFolder).unwrap();
    assert!(missing.is_empty());
}

#[test]
fn find_missing_can_dedupe_all_mail_against_inbox() {
    let tmp = tempfile::tempdir().unwrap();
    let store = MailStore::new(tmp.path());
    ingest_storage_message(
        tmp.path(),
        "inbox",
        "remote_uid:1",
        b"Message-ID: <stable@example.com>\r\nSubject: old\r\n\r\nbody",
    );

    let remote = RemoteMessage {
        uid: 9001,
        uidvalidity: Some(123),
        size: 999,
        rfc_message_id: Some("stable@example.com".to_string()),
        read_state: true,
        starred: false,
    };

    let local_only = find_missing(
        std::slice::from_ref(&remote),
        &store,
        "archive",
        DedupeScope::LocalFolder,
    )
    .unwrap();
    assert_eq!(local_only.len(), 1);

    let all_folders = find_missing(&[remote], &store, "archive", DedupeScope::AllFolders).unwrap();
    assert!(all_folders.is_empty());
}

#[test]
fn find_missing_does_not_scan_legacy_maildir_without_message_id() {
    let tmp = tempfile::tempdir().unwrap();
    let store = MailStore::new(tmp.path());
    let body = b"Subject: no id\r\n\r\nbody";
    store.store_message("inbox", "inbox-7", body).unwrap();

    let remote = RemoteMessage {
        uid: 7,
        uidvalidity: Some(123),
        size: body.len() as u64,
        rfc_message_id: None,
        read_state: false,
        starred: false,
    };

    let missing = find_missing(&[remote], &store, "inbox", DedupeScope::LocalFolder).unwrap();
    assert_eq!(missing.len(), 1);
}

#[test]
fn find_missing_falls_back_to_uid_and_size_with_storage_backed_rows() {
    let tmp = tempfile::tempdir().unwrap();
    let mut storage = Storage::open(tmp.path()).unwrap();
    let body = b"Subject: no id\r\n\r\nbody";
    storage
        .ingest_message(
            &MessageIngestRequest {
                account: "test".into(),
                local_role: "inbox".into(),
                read_state: false,
                starred: false,
                message_id_hint: None,
                seed_hint: "remote_uid:7".into(),
                remote: Some(RemoteBindingInput {
                    account: "test".into(),
                    provider: "protonmail".into(),
                    remote_mailbox: "INBOX".into(),
                    remote_uid: 7,
                    remote_uidvalidity: 123,
                }),
            },
            body,
        )
        .unwrap();
    let store = MailStore::new(tmp.path());

    let remote = RemoteMessage {
        uid: 7,
        uidvalidity: Some(123),
        size: body.len() as u64,
        rfc_message_id: None,
        read_state: false,
        starred: false,
    };

    let missing = find_missing(&[remote], &store, "inbox", DedupeScope::LocalFolder).unwrap();
    assert!(missing.is_empty());
}

#[test]
fn store_message_preserves_remote_read_and_starred_flags() {
    let tmp = tempfile::tempdir().unwrap();
    let account = account_with_provider(Provider::Standard);
    let _store = MailStore::new(tmp.path());
    let mut storage = Storage::open(tmp.path()).unwrap();
    let remote = RemoteMessage {
        uid: 42,
        uidvalidity: Some(123),
        size: 29,
        rfc_message_id: Some("m@example.com".to_string()),
        read_state: true,
        starred: true,
    };

    let entry = store_message(
        &mut storage,
        &account,
        "INBOX",
        "inbox",
        b"Message-ID: <m@example.com>\r\n\r\nbody",
        &remote,
    )
    .unwrap();

    assert!(entry.read_state);
    assert!(entry.starred);
}

#[test]
fn refresh_remote_flags_updates_existing_storage_rows() {
    let tmp = tempfile::tempdir().unwrap();
    let _account = account_with_provider(Provider::Standard);
    let _store = MailStore::new(tmp.path());
    let mut storage = Storage::open(tmp.path()).unwrap();
    let stored = storage
        .ingest_message(
            &MessageIngestRequest {
                account: "test".into(),
                local_role: "inbox".into(),
                read_state: false,
                starred: false,
                message_id_hint: None,
                seed_hint: "remote_uid:7".into(),
                remote: Some(RemoteBindingInput {
                    account: "test".into(),
                    provider: "standard".into(),
                    remote_mailbox: "INBOX".into(),
                    remote_uid: 7,
                    remote_uidvalidity: 123,
                }),
            },
            b"Message-ID: <m@example.com>\r\n\r\nbody",
        )
        .unwrap();
    let remote = RemoteMessage {
        uid: 7,
        uidvalidity: Some(123),
        size: 29,
        rfc_message_id: Some("m@example.com".to_string()),
        read_state: true,
        starred: true,
    };

    refresh_remote_flags(&mut storage, "test", "INBOX", &[remote]).unwrap();
    drop(storage);

    let updated = Storage::open(tmp.path())
        .unwrap()
        .catalog_entry("test", &stored.message_id)
        .unwrap()
        .unwrap();
    assert!(updated.read_state);
    assert!(updated.starred);
}

#[test]
fn protonmail_syncs_all_mail_without_all_flag() {
    let account = account_with_provider(Provider::Protonmail);
    let folders = sync_folders(&account, false);

    assert!(folders.len() == 2);
    assert!(folders.iter().any(|f| f.local_folder == "inbox"));
    assert!(folders.iter().any(|f| f.local_folder == "sent"));
    assert!(!folders.iter().any(|f| f.local_folder == "archive"));
}

#[test]
fn protonmail_syncs_all_mail_into_archive() {
    let account = account_with_provider(Provider::Protonmail);
    let folders = sync_folders(&account, true);

    assert!(
        folders
            .iter()
            .any(|folder| folder.remote_folder == "All Mail" && folder.local_folder == "archive")
    );
}

#[test]
fn standard_provider_no_all_mail_even_with_flag() {
    let account = account_with_provider(Provider::Standard);
    let folders = sync_folders(&account, true);

    assert!(folders.len() == 2);
    assert!(!folders.iter().any(|f| f.local_folder == "archive"));
}

#[test]
fn remote_identity_candidates_preserve_uidvalidity() {
    let account = account_with_provider(Provider::Protonmail);
    let remote = RemoteMessage {
        uid: 42,
        uidvalidity: Some(77),
        size: 100,
        rfc_message_id: Some("m@example.com".to_string()),
        read_state: true,
        starred: true,
    };

    let candidates = remote_identity_candidates(&account, "INBOX", "inbox", &[remote]);

    assert_eq!(candidates.len(), 1);
    assert_eq!(candidates[0].account, "test");
    assert_eq!(candidates[0].provider, "protonmail");
    assert_eq!(candidates[0].remote_mailbox, "INBOX");
    assert_eq!(candidates[0].local_folder, "inbox");
    assert_eq!(candidates[0].uid, 42);
    assert_eq!(candidates[0].uidvalidity, Some(77));
}

fn account_with_provider(provider: Provider) -> Account {
    Account {
        name: "test".into(),
        email: "test@example.com".into(),
        imap_host: "localhost".into(),
        imap_port: Some(1143),
        imap_security: Some(crate::config::Security::Starttls),
        smtp_host: "localhost".into(),
        smtp_port: Some(1025),
        smtp_security: Some(crate::config::Security::Starttls),
        username: "test@example.com".into(),
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
        storage_mode: None,
        provider,
        oauth_authorization_url: None,
        oauth_token_url: None,
        oauth_scope: None,
        reject_invalid_certs: None,
        policy: crate::config::MutationPolicy::FullWrite,
    }
}

fn ingest_storage_message(root: &std::path::Path, local_role: &str, seed_hint: &str, data: &[u8]) {
    Storage::open(root)
        .unwrap()
        .ingest_message(
            &MessageIngestRequest {
                account: "test".into(),
                local_role: local_role.into(),
                read_state: false,
                starred: false,
                message_id_hint: None,
                seed_hint: seed_hint.into(),
                remote: None,
            },
            data,
        )
        .unwrap();
}

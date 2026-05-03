use super::*;

#[test]
fn find_missing_skips_remote_uid_remap_when_message_id_matches() {
    let tmp = tempfile::tempdir().unwrap();
    let store = MailStore::new(tmp.path());
    store
        .store_message(
            "inbox",
            "inbox-1",
            b"Message-ID: <stable@example.com>\r\nSubject: old\r\n\r\nbody",
        )
        .unwrap();

    let remote = RemoteMessage {
        uid: 9001,
        uidvalidity: Some(123),
        size: 999,
        rfc_message_id: Some("stable@example.com".to_string()),
    };

    let missing = find_missing(&[remote], &store, "inbox", DedupeScope::LocalFolder).unwrap();
    assert!(missing.is_empty());
}

#[test]
fn find_missing_can_dedupe_all_mail_against_inbox() {
    let tmp = tempfile::tempdir().unwrap();
    let store = MailStore::new(tmp.path());
    store
        .store_message(
            "inbox",
            "inbox-1",
            b"Message-ID: <stable@example.com>\r\nSubject: old\r\n\r\nbody",
        )
        .unwrap();

    let remote = RemoteMessage {
        uid: 9001,
        uidvalidity: Some(123),
        size: 999,
        rfc_message_id: Some("stable@example.com".to_string()),
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
fn find_missing_falls_back_to_uid_and_size_without_message_id() {
    let tmp = tempfile::tempdir().unwrap();
    let store = MailStore::new(tmp.path());
    let body = b"Subject: no id\r\n\r\nbody";
    store.store_message("inbox", "inbox-7", body).unwrap();

    let remote = RemoteMessage {
        uid: 7,
        uidvalidity: Some(123),
        size: body.len() as u64,
        rfc_message_id: None,
    };

    let missing = find_missing(&[remote], &store, "inbox", DedupeScope::LocalFolder).unwrap();
    assert!(missing.is_empty());
}

#[test]
fn protonmail_syncs_all_mail_without_all_flag() {
    let account = account_with_provider(Provider::Protonmail);
    let folders = sync_folders(&account, false);

    assert!(folders.len() == 2);
    assert!(
        folders.iter().any(|f| f.local_folder == "inbox")
    );
    assert!(
        folders.iter().any(|f| f.local_folder == "sent")
    );
    assert!(
        !folders.iter().any(|f| f.local_folder == "archive")
    );
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
        imap_security: crate::config::Security::Starttls,
        smtp_host: "localhost".into(),
        smtp_port: Some(1025),
        smtp_security: crate::config::Security::Starttls,
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
        provider,
        oauth_authorization_url: None,
        oauth_token_url: None,
        oauth_scope: None,
        reject_invalid_certs: None,
    }
}

use std::path::Path;

use super::*;
use crate::catalog::{Catalog, CatalogEntry, fingerprint};
use crate::store::MailStore;

#[test]
fn normalize_email_extracts_exact_address() {
    assert_eq!(
        normalize_email("Ian <IAN@Example.COM>").as_deref(),
        Some("ian@example.com")
    );
    assert_eq!(
        normalize_email("ian@example.com").as_deref(),
        Some("ian@example.com")
    );
    assert_eq!(normalize_email("not an address"), None);
}

#[test]
fn next_batch_groups_unprocessed_trusted_thread_messages() {
    let tmp = tempfile::tempdir().unwrap();
    let store = MailStore::new(tmp.path());
    let root = store
        .store_message(
            "inbox",
            "root-1",
            b"Message-ID: <root@example.com>\r\nFrom: Ian <ian@example.com>\r\nTo: Agent <agent@example.com>\r\nDate: Sat, 2 May 2026 12:00:00 +0000\r\nSubject: prices\r\n\r\nplease check prices",
        )
        .unwrap();
    let reply = store
        .store_message(
            "inbox",
            "reply-2",
            b"Message-ID: <reply@example.com>\r\nIn-Reply-To: <root@example.com>\r\nReferences: <root@example.com>\r\nFrom: Ian <ian@example.com>\r\nTo: Agent <agent@example.com>\r\nDate: Sat, 2 May 2026 12:01:00 +0000\r\nSubject: Re: prices\r\n\r\nalso include shipping",
        )
        .unwrap();
    let sent = store
        .store_message_in(
            "sent",
            "cur",
            "sent-3",
            b"Message-ID: <sent@example.com>\r\nIn-Reply-To: <reply@example.com>\r\nReferences: <root@example.com> <reply@example.com>\r\nFrom: Agent <agent@example.com>\r\nTo: Ian <ian@example.com>\r\nDate: Sat, 2 May 2026 12:02:00 +0000\r\nSubject: Re: prices\r\n\r\nworking on it",
        )
        .unwrap();
    catalog(tmp.path(), "acct", "root-1", &root, "inbox");
    catalog(tmp.path(), "acct", "reply-2", &reply, "inbox");
    catalog(tmp.path(), "acct", "sent-3", &sent, "sent");
    EmailIndex::rebuild(tmp.path(), "acct").unwrap();

    let ledger = AgentLedger::open(tmp.path()).unwrap();
    let batch = next_batch(
        &store,
        "acct",
        &ledger,
        &AgentPollOptions {
            trusted_from: "IAN@example.com".into(),
            folder: "inbox".into(),
            dry_run: true,
            json: false,
            codex_command: "codex".into(),
            codex_args: vec!["exec".into(), "-".into()],
        },
    )
    .unwrap()
    .unwrap();

    assert_eq!(batch.seed.message_id, "root-1");
    assert_eq!(batch.messages.len(), 3);
    assert_eq!(batch.claimed_message_ids, vec!["root-1", "reply-2"]);

    let prompt = codex_prompt("acct", &batch).unwrap();
    assert!(prompt.contains("send a reply in this same thread"));
    assert!(prompt.contains("summarizing what action you took"));
    assert!(prompt.contains("explaining that no action was taken"));
    assert!(prompt.contains("send from the account below"));
    assert!(prompt.contains("Account: acct"));
}

fn catalog(mail_root: &Path, account: &str, handle: &str, path: &Path, role: &str) {
    let data = std::fs::read(path).unwrap();
    Catalog::open(mail_root)
        .unwrap()
        .upsert(&CatalogEntry {
            handle: handle.to_string(),
            account: account.to_string(),
            content_id: fingerprint(&data),
            blob_path: path.to_string_lossy().to_string(),
            local_role: role.to_string(),
            read_state: false,
            starred: false,
            date: String::new(),
            from: String::new(),
            to: String::new(),
            cc: String::new(),
            bcc: String::new(),
            subject: String::new(),
            rfc_message_id: crate::message::message_id_from_bytes(&data).unwrap_or_default(),
            remote: None,
        })
        .unwrap();
}

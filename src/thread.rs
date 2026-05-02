use std::fs;

use crate::email_index::{self, IndexedMessage};
use crate::error::VivariumError;
use crate::message;
use crate::retrieve::citation_json;
use crate::store::MailStore;

pub fn print_thread_json(
    store: &MailStore,
    account: &str,
    seed_handle: &str,
    limit: usize,
) -> Result<(), VivariumError> {
    let output = thread_json(store, account, seed_handle, limit)?;
    println!(
        "{}",
        serde_json::to_string_pretty(&output).unwrap_or_else(|_| output.to_string())
    );
    Ok(())
}

pub fn thread_json(
    store: &MailStore,
    account: &str,
    seed_handle: &str,
    limit: usize,
) -> Result<serde_json::Value, VivariumError> {
    let index = email_index::ensure_for_thread(store.root(), account, seed_handle)?;
    let messages = index.thread_messages(account, seed_handle, limit)?;
    let total = messages.len();
    let messages = messages
        .into_iter()
        .take(limit)
        .map(|message| indexed_message_json(message, account))
        .collect::<Result<Vec<_>, _>>()?;

    Ok(serde_json::json!({
        "seed": seed_handle,
        "total": total,
        "limit": limit,
        "messages": messages,
    }))
}

fn indexed_message_json(
    indexed: IndexedMessage,
    account: &str,
) -> Result<serde_json::Value, VivariumError> {
    let data = fs::read(&indexed.raw_path)?;
    let mut json = message::to_json_message(&indexed.handle, &data)?;
    json["citation"] = citation_json(&indexed.handle, account, &indexed.location());
    Ok(json)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::{Catalog, CatalogEntry};
    use crate::store::MailStore;
    use std::path::Path;

    #[test]
    fn thread_json_finds_reply_by_indexed_references() {
        let tmp = tempfile::tempdir().unwrap();
        let store = MailStore::new(tmp.path());
        let root = store
            .store_message(
                "inbox",
                "inbox-1",
                b"Message-ID: <root@example.com>\r\nFrom: A <a@example.com>\r\nTo: B <b@example.com>\r\nDate: Sat, 2 May 2026 12:00:00 +0000\r\nSubject: root\r\n\r\nroot body",
            )
            .unwrap();
        let reply = store
            .store_message_in(
                "sent",
                "cur",
                "sent-2",
                b"Message-ID: <reply@example.com>\r\nIn-Reply-To: <root@example.com>\r\nReferences: <root@example.com>\r\nFrom: B <b@example.com>\r\nTo: A <a@example.com>\r\nDate: Sat, 2 May 2026 12:01:00 +0000\r\nSubject: Re: root\r\n\r\nreply body",
            )
            .unwrap();
        catalog(tmp.path(), "acct", "cat-root", &root, "INBOX", "new");
        catalog(tmp.path(), "acct", "cat-reply", &reply, "Sent", "cur");
        crate::email_index::rebuild(tmp.path(), "acct").unwrap();

        let json = thread_json(&store, "acct", "inbox-1", 50).unwrap();

        assert_eq!(json["total"], 2);
        assert_eq!(json["messages"][0]["handle"], "inbox-1");
        assert_eq!(json["messages"][1]["handle"], "sent-2");
        assert_eq!(json["messages"][1]["citation"]["folder"], "Sent");

        let limited = thread_json(&store, "acct", "inbox-1", 1).unwrap();
        assert_eq!(limited["total"], 2);
        assert_eq!(limited["messages"].as_array().unwrap().len(), 1);
    }

    fn catalog(
        mail_root: &Path,
        account: &str,
        handle: &str,
        path: &Path,
        folder: &str,
        subdir: &str,
    ) {
        let data = std::fs::read(path).unwrap();
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
}

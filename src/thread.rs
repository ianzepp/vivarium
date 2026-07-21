use std::fs;

use crate::email_index::{self, IndexedMessage};
use crate::error::VivariumError;
use crate::message;
use crate::retrieve::citation_json;
use crate::store::MailStore;

/// Print thread messages as JSON to stdout.
///
/// # Errors
/// Returns an error if resolving the seed, reading the index, or serializing
/// the output fails.
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

/// Build a JSON representation of a thread.
///
/// # Errors
/// Returns an error if resolving the seed, reading the index, reading message
/// data, or parsing an email fails.
pub fn thread_json(
    store: &MailStore,
    account: &str,
    seed_handle: &str,
    limit: usize,
) -> Result<serde_json::Value, VivariumError> {
    let resolved_seed = store.resolve_message_id(seed_handle)?;
    let index = email_index::ensure_for_thread(store.root(), account, &resolved_seed)?;
    let messages = index.thread_messages(account, &resolved_seed, limit)?;
    let total = messages.len();
    let messages = messages
        .iter()
        .take(limit)
        .map(|message| indexed_message_json(message, account))
        .collect::<Result<Vec<_>, _>>()?;

    Ok(serde_json::json!({
        "seed": store.display_handle(&resolved_seed)?,
        "total": total,
        "limit": limit,
        "messages": messages,
    }))
}

fn indexed_message_json(
    indexed: &IndexedMessage,
    account: &str,
) -> Result<serde_json::Value, VivariumError> {
    let data = fs::read(&indexed.blob_path)?;
    let mut json = message::to_json_message(&indexed.handle, &data)?;
    json["citation"] = citation_json(&indexed.handle, account, &indexed.location());
    Ok(json)
}

#[cfg(test)]
fn storage_role(folder: &str) -> String {
    match folder.to_ascii_lowercase().as_str() {
        "inbox" => "inbox".into(),
        "archive" => "archive".into(),
        "trash" => "trash".into(),
        "sent" => "sent".into(),
        "draft" | "drafts" => "drafts".into(),
        other => other.into(),
    }
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
        catalog(tmp.path(), "acct", "inbox-1", &root, "INBOX", "new");
        catalog(tmp.path(), "acct", "sent-2", &reply, "Sent", "cur");
        crate::email_index::rebuild(tmp.path(), "acct").unwrap();

        let json = thread_json(&store, "acct", "inbox-1", 50).unwrap();

        assert_eq!(json["total"], 2);
        assert_eq!(json["messages"][0]["handle"], "inbox-1");
        assert_eq!(json["messages"][1]["handle"], "sent-2");
        assert_eq!(json["messages"][1]["citation"]["local_role"], "sent");

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
                account: account.to_string(),
                content_id: crate::catalog::fingerprint(&data),
                blob_path: path.to_string_lossy().to_string(),
                local_role: storage_role(folder),
                read_state: subdir == "cur",
                starred: false,
                date: "2026-05-02 12:00".to_string(),
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
}

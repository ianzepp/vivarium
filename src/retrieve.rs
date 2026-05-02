use std::io::{self, Write};

use crate::error::VivariumError;
use crate::extract;
use crate::message;
use crate::store::{MailStore, MessageLocation};

pub fn print_json_messages(
    store: &MailStore,
    account: &str,
    message_ids: &[String],
) -> Result<(), VivariumError> {
    let mut messages = Vec::new();
    for message_id in message_ids {
        messages.push(json_message(store, account, message_id)?);
    }
    println!(
        "{}",
        serde_json::to_string_pretty(&messages).unwrap_or_else(|_| "[]".to_string())
    );
    Ok(())
}

pub fn export_raw_message(store: &MailStore, message_id: &str) -> Result<(), VivariumError> {
    let data = store.read_message(message_id)?;
    io::stdout().write_all(&data)?;
    Ok(())
}

pub fn export_text_message(store: &MailStore, message_id: &str) -> Result<(), VivariumError> {
    let data = store.read_message(message_id)?;
    let extracted = extract::extract_text(&data)?;
    io::stdout().write_all(extracted.body_text.as_bytes())?;
    Ok(())
}

pub fn json_message(
    store: &MailStore,
    account: &str,
    message_id: &str,
) -> Result<serde_json::Value, VivariumError> {
    let location = store.locate_message(message_id)?;
    let data = std::fs::read(&location.path)?;
    let mut value = message::to_json_message(message_id, &data)?;
    value["citation"] = citation_json(message_id, account, &location);
    Ok(value)
}

pub fn citation_json(handle: &str, account: &str, location: &MessageLocation) -> serde_json::Value {
    serde_json::json!({
        "handle": handle,
        "account": account,
        "folder": location.folder,
        "maildir_subdir": location.maildir_subdir,
        "raw_path": location.path.to_string_lossy(),
        "source_type": "rfc5322",
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn json_message_includes_citation() {
        let tmp = tempfile::tempdir().unwrap();
        let store = MailStore::new(tmp.path());
        store
            .store_message(
                "inbox",
                "inbox-1",
                b"Message-ID: <a@example.com>\r\nFrom: A <a@example.com>\r\nTo: B <b@example.com>\r\nSubject: hello\r\n\r\nbody",
            )
            .unwrap();

        let json = json_message(&store, "acct", "inbox-1").unwrap();

        assert_eq!(json["handle"], "inbox-1");
        assert_eq!(json["citation"]["account"], "acct");
        assert_eq!(json["citation"]["folder"], "INBOX");
        assert_eq!(json["citation"]["maildir_subdir"], "new");
        assert!(
            json["citation"]["raw_path"]
                .as_str()
                .unwrap()
                .ends_with("inbox-1.eml")
        );
    }

    #[test]
    fn export_text_uses_extracted_body() {
        let tmp = tempfile::tempdir().unwrap();
        let store = MailStore::new(tmp.path());
        store
            .store_message(
                "inbox",
                "inbox-1",
                b"From: a@example.com\r\nTo: b@example.com\r\nSubject: hello\r\n\r\nbody text",
            )
            .unwrap();

        let data = store.read_message("inbox-1").unwrap();
        let extracted = extract::extract_text(&data).unwrap();

        assert_eq!(extracted.body_text, "body text");
    }
}

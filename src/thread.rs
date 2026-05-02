use std::collections::BTreeSet;

use chrono::DateTime;
use mail_parser::MessageParser;

use crate::error::VivariumError;
use crate::message::{self, normalize_message_id};
use crate::retrieve::citation_json;
use crate::store::{MailStore, MessageLocation};

const THREAD_FOLDERS: &[&str] = &["INBOX", "Archive", "Sent", "Drafts"];

#[derive(Debug, Clone)]
struct ThreadCandidate {
    handle: String,
    location: MessageLocation,
    data: Vec<u8>,
    date: Option<DateTime<chrono::Utc>>,
    message_id: Option<String>,
    related_ids: BTreeSet<String>,
}

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
    let seed_location = store.locate_message(seed_handle)?;
    let seed_data = std::fs::read(&seed_location.path)?;
    let seed = candidate_from_raw(seed_handle, seed_location, seed_data)?;

    let mut thread_ids = seed.related_ids.clone();
    if let Some(message_id) = &seed.message_id {
        thread_ids.insert(message_id.clone());
    }

    let mut candidates = collect_candidates(store)?;
    candidates.retain(|candidate| is_thread_match(candidate, &seed, &thread_ids));
    candidates.sort_by(|a, b| a.date.cmp(&b.date).then_with(|| a.handle.cmp(&b.handle)));

    let total = candidates.len();
    let messages = candidates
        .into_iter()
        .take(limit)
        .map(|candidate| candidate_json(candidate, account))
        .collect::<Result<Vec<_>, _>>()?;

    Ok(serde_json::json!({
        "seed": seed_handle,
        "total": total,
        "limit": limit,
        "messages": messages,
    }))
}

fn candidate_json(
    candidate: ThreadCandidate,
    account: &str,
) -> Result<serde_json::Value, VivariumError> {
    let mut json = message::to_json_message(&candidate.handle, &candidate.data)?;
    json["in_reply_to"] =
        serde_json::json!(candidate.related_ids.iter().cloned().collect::<Vec<_>>());
    json["citation"] = citation_json(&candidate.handle, account, &candidate.location);
    Ok(json)
}

fn collect_candidates(store: &MailStore) -> Result<Vec<ThreadCandidate>, VivariumError> {
    let mut candidates = Vec::new();
    for folder in THREAD_FOLDERS {
        for entry in store.list_messages(folder)? {
            let location = store.locate_message(&entry.message_id)?;
            let data = std::fs::read(&location.path)?;
            if let Ok(candidate) = candidate_from_raw(&entry.message_id, location, data) {
                candidates.push(candidate);
            }
        }
    }
    Ok(candidates)
}

fn candidate_from_raw(
    handle: &str,
    location: MessageLocation,
    data: Vec<u8>,
) -> Result<ThreadCandidate, VivariumError> {
    let parsed = MessageParser::default()
        .parse(&data)
        .ok_or_else(|| VivariumError::Parse(format!("failed to parse message: {handle}")))?;

    let date = parsed
        .date()
        .and_then(|d| DateTime::from_timestamp(d.to_timestamp(), 0));
    let message_id = parsed.message_id().and_then(normalize_message_id);
    let mut related_ids = message_ids_from_header(parsed.in_reply_to());
    related_ids.extend(message_ids_from_header(parsed.references()));

    Ok(ThreadCandidate {
        handle: handle.to_string(),
        location,
        data,
        date,
        message_id,
        related_ids,
    })
}

fn is_thread_match(
    candidate: &ThreadCandidate,
    seed: &ThreadCandidate,
    thread_ids: &BTreeSet<String>,
) -> bool {
    if candidate.handle == seed.handle {
        return true;
    }
    if candidate
        .message_id
        .as_ref()
        .is_some_and(|message_id| thread_ids.contains(message_id))
    {
        return true;
    }
    candidate
        .related_ids
        .iter()
        .any(|message_id| thread_ids.contains(message_id))
}

fn message_ids_from_header(header: &mail_parser::HeaderValue<'_>) -> BTreeSet<String> {
    let mut ids = BTreeSet::new();
    if let Some(values) = header.as_text_list() {
        for value in values {
            for token in value.split_whitespace() {
                if let Some(id) = normalize_message_id(token) {
                    ids.insert(id);
                }
            }
        }
    }
    ids
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::retrieve::json_message;

    #[test]
    fn thread_json_finds_reply_by_references() {
        let tmp = tempfile::tempdir().unwrap();
        let store = MailStore::new(tmp.path());
        store
            .store_message(
                "inbox",
                "inbox-1",
                b"Message-ID: <root@example.com>\r\nFrom: A <a@example.com>\r\nTo: B <b@example.com>\r\nDate: Sat, 2 May 2026 12:00:00 +0000\r\nSubject: root\r\n\r\nroot body",
            )
            .unwrap();
        store
            .store_message(
                "sent",
                "sent-2",
                b"Message-ID: <reply@example.com>\r\nIn-Reply-To: <root@example.com>\r\nReferences: <root@example.com>\r\nFrom: B <b@example.com>\r\nTo: A <a@example.com>\r\nDate: Sat, 2 May 2026 12:01:00 +0000\r\nSubject: Re: root\r\n\r\nreply body",
            )
            .unwrap();

        let json = thread_json(&store, "acct", "inbox-1", 50).unwrap();

        assert_eq!(json["total"], 2);
        assert_eq!(json["messages"][0]["handle"], "inbox-1");
        assert_eq!(json["messages"][1]["handle"], "sent-2");
        assert_eq!(json["messages"][1]["citation"]["folder"], "Sent");
    }

    #[test]
    fn header_message_ids_are_normalized() {
        let header = mail_parser::HeaderValue::Text("<A@example.COM> <b@example.com>".into());
        let ids = message_ids_from_header(&header);

        assert!(ids.contains("a@example.com"));
        assert!(ids.contains("b@example.com"));
    }

    #[test]
    fn json_message_import_remains_usable_for_seed_contract() {
        let tmp = tempfile::tempdir().unwrap();
        let store = MailStore::new(tmp.path());
        store
            .store_message(
                "inbox",
                "inbox-1",
                b"Message-ID: <root@example.com>\r\nFrom: A <a@example.com>\r\nTo: B <b@example.com>\r\nSubject: root\r\n\r\nroot body",
            )
            .unwrap();

        let json = json_message(&store, "acct", "inbox-1").unwrap();

        assert_eq!(json["citation"]["source_type"], "rfc5322");
    }
}

use super::*;
use crate::catalog::CatalogEntry;
use crate::catalog::RemoteIdentity;
use chrono::{DateTime, Utc};
use std::collections::BTreeSet;

fn entry(handle: &str, uid: u32) -> CatalogEntry {
    CatalogEntry {
        handle: handle.into(),
        account: "agent".into(),
        content_id: format!("content-{handle}"),
        blob_path: format!("blobs/{handle}"),
        local_role: "inbox".into(),
        read_state: false,
        starred: false,
        date: "2026-01-01T00:00:00Z".into(),
        from: "sender@example.com".into(),
        to: "agent@example.com".into(),
        cc: String::new(),
        bcc: String::new(),
        subject: format!("subject-{handle}"),
        rfc_message_id: format!("<{handle}@example.com>"),
        remote: Some(RemoteIdentity {
            account: "agent".into(),
            provider: "standard".into(),
            remote_mailbox: "INBOX".into(),
            local_folder: "inbox".into(),
            uid,
            uidvalidity: 7,
            rfc_message_id: format!("<{handle}@example.com>"),
            size: 1,
            content_fingerprint: format!("content-{handle}"),
        }),
    }
}

fn result(entries: Vec<CatalogEntry>) -> SyncResult {
    SyncResult {
        new: entries.len(),
        cataloged_entries: entries,
        ..SyncResult::default()
    }
}

fn entry_with_sender(handle: &str, uid: u32, sender: &str) -> CatalogEntry {
    let mut entry = entry(handle, uid);
    entry.from = sender.into();
    entry
}

#[test]
fn exact_sender_address_is_normalized_from_catalog_metadata() {
    let event = event_from_result(
        "agent",
        &result(vec![entry_with_sender(
            "one",
            1,
            "Alice Example <Alice@Example.COM>",
        )]),
        InboxWatchSource::ImapIdle,
    );
    assert_eq!(
        event.messages[0].sender_address.as_deref(),
        Some("alice@example.com")
    );
}

#[test]
fn malformed_or_missing_sender_metadata_is_explicitly_absent() {
    let event = event_from_result(
        "agent",
        &result(vec![
            entry_with_sender("bad", 1, "not-an-address"),
            entry_with_sender("missing", 2, ""),
        ]),
        InboxWatchSource::Poll,
    );
    assert_eq!(event.messages[0].sender_address, None);
    assert_eq!(event.messages[1].sender_address, None);
}

#[test]
fn event_json_has_no_body_like_fields_or_content() {
    let event = event_from_result(
        "agent",
        &result(vec![entry("one", 1)]),
        InboxWatchSource::ImapIdle,
    );
    let value = serde_json::to_value(event).unwrap();
    assert_no_body_fields(&value);
    let encoded = value.to_string();
    assert!(!encoded.contains("Subject:"));
    assert!(!encoded.contains("body"));
}

fn assert_no_body_fields(value: &serde_json::Value) {
    match value {
        serde_json::Value::Object(map) => {
            for (key, child) in map {
                assert!(!matches!(
                    key.as_str(),
                    "body" | "text" | "html" | "raw" | "content" | "snippet"
                ));
                assert_no_body_fields(child);
            }
        }
        serde_json::Value::Array(values) => {
            for value in values {
                assert_no_body_fields(value);
            }
        }
        _ => {}
    }
}

#[test]
fn event_is_immediately_usable_by_downstream() {
    let event = event_from_result(
        "agent",
        &result(vec![entry("one", 1)]),
        InboxWatchSource::ImapIdle,
    );
    assert_eq!(event.new_count, 1);
    assert_eq!(event.messages[0].event_id, "imap:INBOX:7:1");
    assert_eq!(event.cursor.as_deref(), Some("imap:INBOX:7:1"));
    assert_eq!(event.source, InboxWatchSource::ImapIdle);
}

#[test]
fn duplicate_message_ids_are_deduplicated_in_order() {
    let event = event_from_result(
        "agent",
        &result(vec![
            entry("one", 1),
            entry("duplicate", 1),
            entry("two", 2),
        ]),
        InboxWatchSource::Poll,
    );
    assert_eq!(event.new_count, 2);
    assert_eq!(
        event
            .messages
            .iter()
            .map(|message| message.event_id.as_str())
            .collect::<Vec<_>>(),
        ["imap:INBOX:7:1", "imap:INBOX:7:2"]
    );
}

#[test]
fn restart_reuses_stable_batch_and_message_identities() {
    let first = event_from_result(
        "agent",
        &result(vec![entry("one", 1), entry("two", 2)]),
        InboxWatchSource::ImapIdle,
    );
    let restarted = event_from_result(
        "agent",
        &result(vec![entry("one", 1), entry("two", 2)]),
        InboxWatchSource::Poll,
    );
    assert_eq!(first.batch_id, restarted.batch_id);
    assert_eq!(first.messages, restarted.messages);
    assert_eq!(first.cursor, restarted.cursor);
}

// This is an executable specification for the Ops bridge's policy. Vivi
// emits every source event; the bridge owns this state and persists it.
#[derive(Default)]
struct WakeBridgeModel {
    quiet_until: Option<DateTime<Utc>>,
    pending: Vec<String>,
    delivered: BTreeSet<String>,
    emitted: Vec<Vec<String>>,
}

impl WakeBridgeModel {
    fn observe(&mut self, event: &InboxWatchEvent, now: DateTime<Utc>) {
        let mut seen = self
            .pending
            .iter()
            .chain(self.delivered.iter())
            .cloned()
            .collect::<BTreeSet<_>>();
        let mut added = false;
        for message in &event.messages {
            if seen.insert(message.event_id.clone()) {
                self.pending.push(message.event_id.clone());
                added = true;
            }
        }
        if !added {
            return;
        }
        let leading = self.quiet_until.is_none_or(|deadline| now >= deadline);
        if leading {
            let wake = std::mem::take(&mut self.pending);
            self.delivered.extend(wake.iter().cloned());
            self.emitted.push(wake);
        }
        self.quiet_until = Some(now + chrono::Duration::seconds(60));
    }

    fn flush(&mut self, now: DateTime<Utc>) {
        if self.quiet_until.is_some_and(|deadline| now >= deadline) && !self.pending.is_empty() {
            let wake = std::mem::take(&mut self.pending);
            self.delivered.extend(wake.iter().cloned());
            self.emitted.push(wake);
            self.quiet_until = None;
        }
    }
}

fn event_at(ids: &[(&str, u32)], at: DateTime<Utc>) -> InboxWatchEvent {
    let mut event = event_from_result(
        "agent",
        &result(
            ids.iter()
                .map(|(handle, uid)| entry(handle, *uid))
                .collect(),
        ),
        InboxWatchSource::ImapIdle,
    );
    event.observed_at = at.to_rfc3339();
    event
}

#[test]
fn downstream_leading_and_trailing_debounce_contract() {
    let start = Utc::now();
    let mut bridge = WakeBridgeModel::default();
    bridge.observe(&event_at(&[("one", 1)], start), start);
    bridge.observe(
        &event_at(&[("two", 2)], start + chrono::Duration::seconds(10)),
        start + chrono::Duration::seconds(10),
    );
    bridge.flush(start + chrono::Duration::seconds(69));
    assert_eq!(bridge.emitted.len(), 1);
    bridge.flush(start + chrono::Duration::seconds(70));
    assert_eq!(
        bridge.emitted,
        vec![
            vec![String::from("imap:INBOX:7:1")],
            vec![String::from("imap:INBOX:7:2")]
        ]
    );
}

#[test]
fn repeated_arrivals_extend_quiet_window_and_boundary_is_inclusive() {
    let start = Utc::now();
    let mut bridge = WakeBridgeModel::default();
    bridge.observe(&event_at(&[("one", 1)], start), start);
    bridge.observe(
        &event_at(&[("two", 2)], start + chrono::Duration::seconds(59)),
        start + chrono::Duration::seconds(59),
    );
    bridge.flush(start + chrono::Duration::seconds(118));
    assert_eq!(bridge.emitted.len(), 1);
    bridge.flush(start + chrono::Duration::seconds(119));
    assert_eq!(bridge.emitted.len(), 2);
}

#[test]
fn duplicate_ids_do_not_duplicate_trailing_wake() {
    let start = Utc::now();
    let mut bridge = WakeBridgeModel::default();
    bridge.observe(&event_at(&[("one", 1)], start), start);
    bridge.observe(
        &event_at(
            &[("one", 1), ("two", 2)],
            start + chrono::Duration::seconds(1),
        ),
        start + chrono::Duration::seconds(1),
    );
    bridge.flush(start + chrono::Duration::seconds(61));
    assert_eq!(
        bridge.emitted,
        vec![
            vec![String::from("imap:INBOX:7:1")],
            vec![String::from("imap:INBOX:7:2")]
        ]
    );
}

#[test]
fn restart_during_window_keeps_pending_ids_without_replaying_leading_wake() {
    let start = Utc::now();
    let mut before_restart = WakeBridgeModel::default();
    before_restart.observe(&event_at(&[("one", 1)], start), start);
    before_restart.observe(
        &event_at(&[("two", 2)], start + chrono::Duration::seconds(10)),
        start + chrono::Duration::seconds(10),
    );
    let pending = before_restart.pending.clone();
    let deadline = before_restart.quiet_until.unwrap();

    let mut after_restart = WakeBridgeModel {
        quiet_until: Some(deadline),
        pending,
        delivered: [String::from("imap:INBOX:7:1")].into_iter().collect(),
        emitted: Vec::new(),
    };
    after_restart.observe(
        &event_at(&[("two", 2)], start + chrono::Duration::seconds(20)),
        start + chrono::Duration::seconds(20),
    );
    after_restart.flush(start + chrono::Duration::seconds(70));
    assert_eq!(
        after_restart.emitted,
        vec![vec![String::from("imap:INBOX:7:2")]]
    );
}

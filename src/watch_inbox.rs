use std::collections::HashSet;
use std::io::{self, Write};
use std::time::Duration;

use chrono::Utc;
use serde::Serialize;

use crate::config::{Account, Config, Provider};
use crate::error::VivariumError;
use crate::imap::InboxWaitMode;
use crate::sync::{SyncResult, SyncWindow};

const POLL_INTERVAL: Duration = Duration::from_secs(30);
const MAX_BACKOFF: Duration = Duration::from_secs(300);

/// Structured, inbound-only notification emitted after mail is synced locally.
///
/// This is deliberately an event source, not a wake policy. Consumers own
/// leading-edge delivery, trailing debounce, and restart checkpoints.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct InboxWatchEvent {
    pub schema: u8,
    pub kind: &'static str,
    pub account: String,
    pub source: InboxWatchSource,
    pub observed_at: String,
    pub batch_id: String,
    pub new_count: usize,
    pub messages: Vec<InboxMessageIdentity>,
    pub cursor: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct InboxMessageIdentity {
    pub message_id: String,
    pub event_id: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum InboxWatchSource {
    ImapIdle,
    Poll,
}

pub async fn watch_inbox(
    account: &Account,
    config: &Config,
    insecure: bool,
) -> Result<(), VivariumError> {
    if matches!(account.provider, Provider::ProtonApi) {
        return Err(VivariumError::Config(format!(
            "account '{}' uses provider = \"proton-api\"; watch-inbox requires IMAP",
            account.name
        )));
    }

    let mut backoff = Duration::from_secs(1);
    loop {
        let source = match crate::imap::wait_for_inbox_change(
            account,
            account.reject_invalid_certs(config) && !insecure,
            POLL_INTERVAL,
        )
        .await
        {
            Ok(source) => {
                backoff = Duration::from_secs(1);
                source
            }
            Err(error) => {
                tracing::warn!(
                    account = account.name,
                    %error,
                    delay_secs = backoff.as_secs(),
                    "inbound watch disconnected"
                );
                tokio::time::sleep(backoff).await;
                backoff = (backoff * 2).min(MAX_BACKOFF);
                continue;
            }
        };

        match crate::sync::sync_inbox_account(account, config, insecure, SyncWindow::default())
            .await
        {
            Ok(result) if result.new > 0 => {
                emit_event(&event_from_result(
                    &account.name,
                    &result,
                    match source {
                        InboxWaitMode::ImapIdle => InboxWatchSource::ImapIdle,
                        InboxWaitMode::Poll => InboxWatchSource::Poll,
                    },
                ))?;
            }
            Ok(_) => {}
            Err(error) => {
                tracing::warn!(account = account.name, %error, "inbound watch sync failed");
                tokio::time::sleep(backoff).await;
                backoff = (backoff * 2).min(MAX_BACKOFF);
            }
        }
    }
}

fn emit_event(event: &InboxWatchEvent) -> Result<(), VivariumError> {
    let mut stdout = io::stdout().lock();
    serde_json::to_writer(&mut stdout, event)
        .map_err(|error| VivariumError::Other(format!("failed to encode watch event: {error}")))?;
    stdout.write_all(b"\n").map_err(VivariumError::Io)?;
    stdout.flush().map_err(VivariumError::Io)
}

fn event_from_result(
    account: &str,
    result: &SyncResult,
    source: InboxWatchSource,
) -> InboxWatchEvent {
    let mut seen = HashSet::new();
    let messages = result
        .cataloged_entries
        .iter()
        .filter_map(|entry| {
            let event_id = entry_event_id(entry);
            if !seen.insert(event_id.clone()) {
                return None;
            }
            Some(InboxMessageIdentity {
                message_id: entry.handle.clone(),
                event_id,
            })
        })
        .collect::<Vec<_>>();
    let cursor = messages.last().map(|message| message.event_id.clone());
    let batch_id = format!(
        "{}:{}",
        account,
        messages
            .iter()
            .map(|message| message.event_id.as_str())
            .collect::<Vec<_>>()
            .join(",")
    );

    InboxWatchEvent {
        schema: 1,
        kind: "inbound_mail",
        account: account.to_string(),
        source,
        observed_at: Utc::now().to_rfc3339(),
        batch_id,
        new_count: messages.len(),
        messages,
        cursor,
    }
}

fn entry_event_id(entry: &crate::catalog::CatalogEntry) -> String {
    entry
        .remote
        .as_ref()
        .map(|remote| {
            format!(
                "imap:{}:{}:{}",
                remote.remote_mailbox, remote.uidvalidity, remote.uid
            )
        })
        .unwrap_or_else(|| format!("message:{}:{}", entry.account, entry.handle))
}

#[cfg(test)]
mod tests {
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
            if self.quiet_until.is_some_and(|deadline| now >= deadline) && !self.pending.is_empty()
            {
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
}

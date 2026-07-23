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
const MAX_BACKOFF: Duration = Duration::from_mins(5);

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
    pub sender_address: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum InboxWatchSource {
    ImapIdle,
    Poll,
}

/// Watch for new mail in an account's inbox, syncing and emitting events.
///
/// # Errors
/// Returns an error if the account uses the Proton API (which requires IMAP
/// for watch-inbox) or if I/O errors occur during event emission.
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
                sender_address: sender_address(&entry.from),
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

fn sender_address(value: &str) -> Option<String> {
    if value.len() > 512 || value.contains(['\r', '\n', '\0']) {
        return None;
    }
    let envelope = format!("From: {value}\r\n\r\n");
    let parsed = mail_parser::MessageParser::default().parse(envelope.as_bytes())?;
    let address = parsed.from()?.first()?.address()?.trim();
    if address.len() > 320 || address.chars().any(char::is_whitespace) || !address.contains('@') {
        return None;
    }
    Some(address.to_ascii_lowercase())
}

fn entry_event_id(entry: &crate::catalog::CatalogEntry) -> String {
    entry.remote.as_ref().map_or_else(
        || format!("message:{}:{}", entry.account, entry.handle),
        |remote| {
            format!(
                "imap:{}:{}:{}",
                remote.remote_mailbox, remote.uidvalidity, remote.uid
            )
        },
    )
}

#[cfg(test)]
#[path = "watch_inbox_test.rs"]
mod tests;

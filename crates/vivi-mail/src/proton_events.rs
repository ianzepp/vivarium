use std::fs::{self, OpenOptions};
use std::io::Write;
#[cfg(unix)]
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::config::{Account, Provider};
use crate::error::VivariumError;
use crate::proton_api::{ProtonApiClient, ProtonEventAction, ProtonSessionStore};
use crate::store::{MailStore, secure_create_dir_all};
use crate::sync::SyncWindow;

#[derive(Debug, Clone, Default, Serialize)]
pub struct ProtonEventSyncReport {
    pub account: String,
    pub previous_event_id: Option<String>,
    pub event_id: Option<String>,
    pub bootstrapped: bool,
    pub events: usize,
    pub created: usize,
    pub updated: usize,
    pub deleted: usize,
    pub full_refreshes: usize,
    pub synced: usize,
    pub decryption_errors: usize,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct ProtonEventSyncOptions {
    pub bootstrap: bool,
}

/// Syncs events from the Proton API for the given account.
///
/// # Errors
/// Returns an error if the account uses a non-Proton API provider, the session cannot be loaded,
/// or any API call fails.
pub async fn sync_events(
    account: &Account,
    store: &MailStore,
    options: ProtonEventSyncOptions,
) -> Result<ProtonEventSyncReport, VivariumError> {
    require_proton_api(account)?;

    let cursor_store = ProtonEventCursorStore::new(store.root());
    let session_store = ProtonSessionStore::new(store.root());
    let mut session = session_store.load()?;
    let client = ProtonApiClient::default();
    let previous_event_id = cursor_store.load()?;
    let mut report = ProtonEventSyncReport {
        account: account.name.clone(),
        previous_event_id: previous_event_id.clone(),
        ..ProtonEventSyncReport::default()
    };

    bootstrap_if_requested(account, store, options, &mut report).await?;

    let Some(mut cursor) = previous_event_id else {
        let (refreshed, latest) = client.latest_event_id(&session).await?;
        session = refreshed;
        session_store.save(&session)?;
        cursor_store.save(&latest)?;
        report.event_id = Some(latest);
        report.bootstrapped = true;
        return Ok(report);
    };

    loop {
        let (refreshed, event, more) = client.event(&session, &cursor).await?;
        session = refreshed;
        session_store.save(&session)?;

        if event.event_id.is_empty() || event.event_id == cursor {
            report.event_id = Some(cursor);
            break;
        }

        report.events += 1;
        cursor = event.event_id.clone();
        apply_event(account, store, event, &mut report).await?;

        cursor_store.save(&cursor)?;
        report.event_id = Some(cursor.clone());

        if !more {
            break;
        }
    }

    Ok(report)
}

fn require_proton_api(account: &Account) -> Result<(), VivariumError> {
    if account.provider == Provider::ProtonApi {
        return Ok(());
    }
    Err(VivariumError::Config(format!(
        "account '{}' uses provider = \"{}\"; sync-events requires provider = \"proton-api\"",
        account.name, account.provider
    )))
}

async fn bootstrap_if_requested(
    account: &Account,
    store: &MailStore,
    options: ProtonEventSyncOptions,
    report: &mut ProtonEventSyncReport,
) -> Result<(), VivariumError> {
    if !options.bootstrap {
        return Ok(());
    }
    let sync =
        crate::proton_sync::sync_messages(account, store, None, SyncWindow::default()).await?;
    report.synced += sync.new;
    report.decryption_errors += sync.decryption_errors;
    report.full_refreshes += 1;
    Ok(())
}

async fn apply_event(
    account: &Account,
    store: &MailStore,
    event: crate::proton_api::ProtonEvent,
    report: &mut ProtonEventSyncReport,
) -> Result<(), VivariumError> {
    if event.requires_mail_refresh() {
        return apply_full_refresh(account, store, report).await;
    }
    apply_message_events(account, store, event.messages, report).await
}

async fn apply_full_refresh(
    account: &Account,
    store: &MailStore,
    report: &mut ProtonEventSyncReport,
) -> Result<(), VivariumError> {
    let sync =
        crate::proton_sync::sync_messages(account, store, None, SyncWindow::default()).await?;
    report.synced += sync.new;
    report.decryption_errors += sync.decryption_errors;
    report.full_refreshes += 1;
    Ok(())
}

async fn apply_message_events(
    account: &Account,
    store: &MailStore,
    messages: Vec<crate::proton_api::ProtonMessageEvent>,
    report: &mut ProtonEventSyncReport,
) -> Result<(), VivariumError> {
    let mut sync_ids = Vec::new();
    for message in messages {
        match message.action {
            ProtonEventAction::Delete => {
                if crate::proton_sync::delete_message_id(account, store, &message.id)? {
                    report.deleted += 1;
                }
            }
            ProtonEventAction::Create => {
                report.created += 1;
                sync_ids.push(message.id);
            }
            ProtonEventAction::Update | ProtonEventAction::UpdateFlags => {
                report.updated += 1;
                sync_ids.push(message.id);
            }
        }
    }
    sync_ids.sort();
    sync_ids.dedup();
    let sync = crate::proton_sync::sync_message_ids(account, store, &sync_ids).await?;
    report.synced += sync.new;
    report.decryption_errors += sync.decryption_errors;
    Ok(())
}

#[derive(Debug, Clone, Deserialize, Serialize)]
struct ProtonEventCursor {
    event_id: String,
}

struct ProtonEventCursorStore {
    path: PathBuf,
}

impl ProtonEventCursorStore {
    fn new(mail_root: &Path) -> Self {
        Self {
            path: mail_root.join(".vivarium").join("proton-events.json"),
        }
    }

    fn load(&self) -> Result<Option<String>, VivariumError> {
        if !self.path.exists() {
            return Ok(None);
        }
        let data = fs::read_to_string(&self.path)?;
        let cursor: ProtonEventCursor = serde_json::from_str(&data)
            .map_err(|e| VivariumError::Parse(format!("invalid Proton event cursor file: {e}")))?;
        Ok(Some(cursor.event_id))
    }

    fn save(&self, event_id: &str) -> Result<(), VivariumError> {
        let Some(parent) = self.path.parent() else {
            return Err(VivariumError::Other(
                "Proton event cursor path has no parent".into(),
            ));
        };
        secure_create_dir_all(parent)?;
        let mut options = OpenOptions::new();
        options.create(true).truncate(true).write(true);
        #[cfg(unix)]
        options.mode(0o600);
        let mut file = options.open(&self.path)?;
        let json = serde_json::to_string_pretty(&ProtonEventCursor {
            event_id: event_id.to_string(),
        })
        .map_err(|e| {
            VivariumError::Other(format!("Proton event cursor serialization failed: {e}"))
        })?;
        file.write_all(json.as_bytes())?;
        file.write_all(b"\n")?;
        file.sync_all()?;
        #[cfg(unix)]
        fs::set_permissions(&self.path, fs::Permissions::from_mode(0o600))?;
        Ok(())
    }
}

use std::time::Duration;

use crate::config::{Account, Config};
use crate::error::VivariumError;
use crate::store::MailStore;
use crate::sync::SyncWindow;

pub async fn watch_all(
    accounts: &[Account],
    config: &Config,
    insecure: bool,
) -> Result<(), VivariumError> {
    let mut handles = Vec::new();
    for account in accounts {
        let account = account.clone();
        let config = config.clone();
        handles.push(tokio::spawn(async move {
            watch_account(&account, &config, insecure).await
        }));
    }

    for handle in handles {
        handle
            .await
            .map_err(|e| VivariumError::Other(format!("watch task failed: {e}")))??;
    }
    Ok(())
}

pub async fn watch_account(
    account: &Account,
    config: &Config,
    insecure: bool,
) -> Result<(), VivariumError> {
    let store = MailStore::new(&account.mail_path(config));
    store.ensure_folders()?;
    let reject_invalid_certs = account.reject_invalid_certs(config) && !insecure;

    tokio::select! {
        result = watch_imap(account.clone(), config.clone(), insecure, reject_invalid_certs) => result,
        result = crate::outbox::watch_outbox(account, &store, reject_invalid_certs) => result,
    }
}

async fn watch_imap(
    account: Account,
    config: Config,
    insecure: bool,
    reject_invalid_certs: bool,
) -> Result<(), VivariumError> {
    let mut backoff = Duration::from_secs(1);
    loop {
        match crate::imap::idle(&account, reject_invalid_certs).await {
            Ok(()) => {
                backoff = Duration::from_secs(1);
                let result = crate::sync::sync_account(
                    &account,
                    &config,
                    insecure,
                    None,
                    SyncWindow::default(),
                )
                .await?;
                tracing::info!(
                    account = account.name,
                    new = result.new,
                    "watch sync complete"
                );
            }
            Err(err) => {
                tracing::warn!(
                    account = account.name,
                    error = %err,
                    delay_secs = backoff.as_secs(),
                    "IMAP watch disconnected"
                );
                tokio::time::sleep(backoff).await;
                backoff = (backoff * 2).min(Duration::from_secs(300));
            }
        }
    }
}

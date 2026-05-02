use crate::config::{Account, Config};
use crate::error::VivariumError;
use crate::store::MailStore;

#[derive(Debug, Default)]
pub struct SyncResult {
    pub new: usize,
    pub archived: usize,
}

pub async fn sync_account(
    account: &Account,
    config: &Config,
    insecure: bool,
    limit: Option<usize>,
) -> Result<SyncResult, VivariumError> {
    let store = MailStore::new(&account.mail_path(config));
    store.ensure_folders()?;

    let reject_invalid_certs = account.reject_invalid_certs(config) && !insecure;
    let result = crate::imap::sync_messages(account, &store, reject_invalid_certs, limit).await?;

    tracing::info!(account = account.name, new = result.new, "sync complete");
    Ok(result)
}

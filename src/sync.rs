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
) -> Result<SyncResult, VivariumError> {
    let store = MailStore::new(&account.mail_path(config));
    store.ensure_folders()?;

    crate::imap::sync_messages(account, &store).await?;

    tracing::info!(account = account.name, "sync complete (stub)");
    Ok(SyncResult::default())
}

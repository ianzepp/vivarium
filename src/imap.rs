use crate::config::Account;
use crate::error::VivariumError;
use crate::store::MailStore;

pub async fn connect(_account: &Account) -> Result<(), VivariumError> {
    tracing::info!("IMAP connect stub");
    Ok(())
}

/// Sync messages from the account's IMAP server into the local store.
pub async fn sync_messages(
    _account: &Account,
    _store: &MailStore,
) -> Result<(), VivariumError> {
    tracing::info!(
        folder = _account.all_mail_folder(),
        "IMAP sync_messages stub"
    );
    Ok(())
}

pub async fn idle(_account: &Account) -> Result<(), VivariumError> {
    tracing::info!("IMAP idle stub");
    Ok(())
}

use crate::config::Account;
use crate::error::VivariumError;

pub async fn watch_all(_accounts: &[Account]) -> Result<(), VivariumError> {
    tracing::info!("watch_all stub");
    Ok(())
}

pub async fn watch_account(_account: &Account) -> Result<(), VivariumError> {
    tracing::info!("watch_account stub");
    Ok(())
}

use std::path::Path;

use crate::config::Account;
use crate::error::VivariumError;

pub async fn watch_outbox(
    _account: &Account,
    _outbox_path: &Path,
) -> Result<(), VivariumError> {
    tracing::info!("watch_outbox stub");
    Ok(())
}

pub async fn process_entry(
    _account: &Account,
    _path: &Path,
) -> Result<(), VivariumError> {
    tracing::info!("process_entry stub");
    Ok(())
}

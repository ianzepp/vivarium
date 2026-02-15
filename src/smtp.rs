use std::path::Path;

use crate::config::Account;
use crate::error::VivariumError;

pub async fn send_message(
    _account: &Account,
    _path: &Path,
) -> Result<(), VivariumError> {
    tracing::info!("SMTP send_message stub");
    Ok(())
}

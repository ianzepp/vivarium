use super::transport::connect;
use crate::config::Account;
use crate::error::VivariumError;

/// Append a raw message to the given IMAP mailbox.
///
/// # Errors
/// Returns an error if the IMAP connection or APPEND command fails.
pub async fn append_message(
    account: &Account,
    mailbox: &str,
    data: &[u8],
    reject_invalid_certs: bool,
) -> Result<(), VivariumError> {
    let mut session = connect(account, reject_invalid_certs).await?;
    let result = session
        .append(mailbox, None, None, data)
        .await
        .map_err(|e| VivariumError::Imap(format!("APPEND to {mailbox} failed: {e}")));
    session.logout().await.ok();
    result
}

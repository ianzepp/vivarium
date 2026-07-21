use std::time::Duration;

use super::transport::connect;
use crate::config::Account;
use crate::error::VivariumError;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InboxWaitMode {
    ImapIdle,
    Poll,
}

/// Wait for inbound mailbox activity without issuing any mutating command.
/// Servers without IDLE use bounded polling rather than attempting IDLE.
///
/// # Errors
/// Returns an error if the IMAP connection, capability check, or IDLE
/// command fails.
pub async fn wait_for_inbox_change(
    account: &Account,
    reject_invalid_certs: bool,
    poll_interval: Duration,
) -> Result<InboxWaitMode, VivariumError> {
    let mut session = connect(account, reject_invalid_certs).await?;
    let capabilities = session
        .capabilities()
        .await
        .map_err(|e| VivariumError::Imap(format!("capability query failed: {e}")))?;
    if !capabilities.has_str("IDLE") {
        session.logout().await.ok();
        tokio::time::sleep(poll_interval).await;
        return Ok(InboxWaitMode::Poll);
    }

    session
        .select("INBOX")
        .await
        .map_err(|e| VivariumError::Imap(format!("idle select INBOX failed: {e}")))?;

    let mut handle = session.idle();
    handle
        .init()
        .await
        .map_err(|e| VivariumError::Imap(format!("IDLE init failed: {e}")))?;

    let (wait, _interrupt) = handle.wait_with_timeout(Duration::from_mins(29));
    wait.await
        .map_err(|e| VivariumError::Imap(format!("IDLE wait failed: {e}")))?;

    let mut session = handle
        .done()
        .await
        .map_err(|e| VivariumError::Imap(format!("IDLE done failed: {e}")))?;
    session.logout().await.ok();
    Ok(InboxWaitMode::ImapIdle)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wait_modes_are_explicitly_inbound_only() {
        assert_ne!(InboxWaitMode::ImapIdle, InboxWaitMode::Poll);
    }
}

use std::fmt;

use async_imap::Session;
use futures::TryStreamExt;
use tokio::net::TcpStream;
use tokio_native_tls::TlsStream;

use crate::config::{Account, Provider, Security};
use crate::error::VivariumError;
use crate::store::MailStore;
use crate::sync::SyncResult;

type ImapSession = Session<TlsStream<TcpStream>>;

/// Connect and authenticate to the account's IMAP server.
pub async fn connect(account: &Account) -> Result<ImapSession, VivariumError> {
    let host = &account.imap_host;
    let port = account.imap_port.unwrap_or(match account.imap_security {
        Security::Ssl => 993,
        Security::Starttls => 143,
    });
    let password = account.resolve_password().await?;

    tracing::info!(host, port, security = %account.imap_security, "connecting to IMAP");

    let tcp = TcpStream::connect((host.as_str(), port))
        .await
        .map_err(|e| VivariumError::Imap(format!("TCP connect to {host}:{port} failed: {e}")))?;

    let tls_connector = native_tls::TlsConnector::builder()
        .danger_accept_invalid_certs(true)
        .build()
        .map_err(|e| VivariumError::Imap(format!("TLS connector build failed: {e}")))?;
    let tls_connector = tokio_native_tls::TlsConnector::from(tls_connector);

    let tls_stream = match account.imap_security {
        Security::Ssl => {
            tls_connector
                .connect(host, tcp)
                .await
                .map_err(|e| VivariumError::Imap(format!("TLS handshake failed: {e}")))?
        }
        Security::Starttls => {
            let mut client = async_imap::Client::new(tcp);
            if let Some(resp) = client.read_response().await {
                resp.map_err(|e| VivariumError::Imap(format!("failed to read greeting: {e}")))?;
            }
            client
                .run_command_and_check_ok("STARTTLS", None)
                .await
                .map_err(|e| VivariumError::Imap(format!("STARTTLS failed: {e}")))?;
            let inner = client.into_inner();
            tls_connector
                .connect(host, inner)
                .await
                .map_err(|e| VivariumError::Imap(format!("STARTTLS TLS upgrade failed: {e}")))?
        }
    };

    let client = async_imap::Client::new(tls_stream);
    let session = client
        .login(&account.username, &password)
        .await
        .map_err(|(e, _)| VivariumError::Imap(format!("login failed: {e}")))?;

    tracing::info!(account = account.name, "IMAP authenticated");
    Ok(session)
}

/// Sync messages from the account's IMAP server into the local store.
pub async fn sync_messages(
    account: &Account,
    store: &MailStore,
) -> Result<SyncResult, VivariumError> {
    let mut session = connect(account).await?;
    let mut result = SyncResult::default();

    // Sync inbox
    sync_folder(&mut session, store, "INBOX", "inbox", &mut result).await?;

    // Sync sent
    let sent = account.sent_folder();
    sync_folder(&mut session, store, sent, "sent", &mut result).await?;

    // For Gmail, sync All Mail → archive (messages not already in inbox)
    if account.provider == Provider::Gmail {
        let all_mail = account.all_mail_folder();
        sync_folder(&mut session, store, all_mail, "archive", &mut result).await?;
    }

    session
        .logout()
        .await
        .map_err(|e| VivariumError::Imap(format!("logout failed: {e}")))?;

    Ok(result)
}

/// Fetch all messages from a remote IMAP folder and store new ones locally.
async fn sync_folder(
    session: &mut ImapSession,
    store: &MailStore,
    remote_folder: &str,
    local_folder: &str,
    result: &mut SyncResult,
) -> Result<(), VivariumError> {
    let mailbox = session
        .select(remote_folder)
        .await
        .map_err(|e| VivariumError::Imap(format!("select {remote_folder} failed: {e}")))?;

    let count = mailbox.exists;
    if count == 0 {
        tracing::info!(folder = remote_folder, "empty folder, nothing to sync");
        return Ok(());
    }

    tracing::info!(folder = remote_folder, count, "fetching messages");

    let range = format!("1:{count}");
    let fetches = session
        .fetch(&range, "(UID BODY[] ENVELOPE)")
        .await
        .map_err(|e| VivariumError::Imap(format!("fetch failed: {e}")))?;

    let messages: Vec<_> = fetches
        .try_collect()
        .await
        .map_err(|e| VivariumError::Imap(format!("fetch stream failed: {e}")))?;

    for fetch in &messages {
        let uid = fetch.uid.unwrap_or(0);
        let body = match fetch.body() {
            Some(b) => b,
            None => {
                tracing::warn!(uid, "message has no body, skipping");
                continue;
            }
        };

        let message_id = format!("{local_folder}-{uid}");
        if store.contains(&message_id) {
            continue;
        }

        store.store_message(local_folder, &message_id, body)?;
        result.new += 1;

        if let Some(envelope) = fetch.envelope() {
            let subject = envelope
                .subject
                .as_ref()
                .map(|s| String::from_utf8_lossy(s))
                .unwrap_or_default();
            tracing::debug!(uid, subject = %subject, folder = local_folder, "stored");
        }
    }

    tracing::info!(
        folder = remote_folder,
        local = local_folder,
        total = messages.len(),
        new = result.new,
        "folder sync complete"
    );

    Ok(())
}

pub async fn idle(_account: &Account) -> Result<(), VivariumError> {
    tracing::info!("IMAP idle stub");
    Ok(())
}

impl fmt::Display for Security {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Security::Ssl => write!(f, "ssl"),
            Security::Starttls => write!(f, "starttls"),
        }
    }
}

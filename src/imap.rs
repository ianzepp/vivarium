use std::fmt;
use std::sync::Arc;
use std::time::Duration;

use async_imap::extensions::idle::IdleResponse;
use async_imap::{Authenticator, Session};
use futures::TryStreamExt;
use tokio::net::TcpStream;
use tokio::sync::Mutex;
use tokio_native_tls::TlsStream;

use crate::config::{Account, Auth, Provider, Security};
use crate::error::VivariumError;
use crate::message::{message_id_from_bytes, normalize_message_id};
use crate::store::MailStore;
use crate::sync::SyncResult;

type ImapSession = Session<TlsStream<TcpStream>>;

const CHUNK_SIZE: u32 = 100;
const WORKER_COUNT: usize = 4;

#[derive(Debug, Clone)]
struct RemoteMessage {
    uid: u32,
    size: u64,
    rfc_message_id: Option<String>,
}

struct WorkerContext {
    worker_id: usize,
    account: Account,
    remote_folder: String,
    local_folder: String,
    store: Arc<MailStore>,
    reject_invalid_certs: bool,
}

struct Xoauth2 {
    user: String,
    access_token: String,
}

impl Authenticator for Xoauth2 {
    type Response = String;

    fn process(&mut self, _challenge: &[u8]) -> Self::Response {
        xoauth2_initial_response(&self.user, &self.access_token)
    }
}

/// Connect and authenticate to the account's IMAP server.
pub async fn connect(
    account: &Account,
    reject_invalid_certs: bool,
) -> Result<ImapSession, VivariumError> {
    let host = &account.imap_host;
    let port = account.imap_port.unwrap_or(match account.imap_security {
        Security::Ssl => 993,
        Security::Starttls => 143,
    });
    let secret = account.resolve_secret().await?;

    tracing::debug!(host, port, security = %account.imap_security, "connecting to IMAP");

    let tcp = TcpStream::connect((host.as_str(), port))
        .await
        .map_err(|e| VivariumError::Imap(format!("TCP connect to {host}:{port} failed: {e}")))?;

    let mut tls_builder = native_tls::TlsConnector::builder();
    if !reject_invalid_certs {
        tls_builder.danger_accept_invalid_certs(true);
    }
    let tls_connector = tls_builder
        .build()
        .map_err(|e| VivariumError::Tls(format!("TLS connector build failed: {e}")))?;
    let tls_connector = tokio_native_tls::TlsConnector::from(tls_connector);

    let tls_stream = match account.imap_security {
        Security::Ssl => tls_connector
            .connect(host, tcp)
            .await
            .map_err(|e| VivariumError::Tls(format!("TLS handshake failed: {e}")))?,
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
                .map_err(|e| VivariumError::Tls(format!("STARTTLS TLS upgrade failed: {e}")))?
        }
    };

    let client = async_imap::Client::new(tls_stream);
    let session = match account.auth {
        Auth::Password => client
            .login(&account.username, &secret)
            .await
            .map_err(|(e, _)| VivariumError::Imap(format!("login failed: {e}")))?,
        Auth::Xoauth2 => client
            .authenticate(
                "XOAUTH2",
                Xoauth2 {
                    user: account.username.clone(),
                    access_token: secret,
                },
            )
            .await
            .map_err(|(e, _)| VivariumError::Imap(format!("XOAUTH2 failed: {e}")))?,
    };

    tracing::debug!(account = account.name, "IMAP authenticated");
    Ok(session)
}

/// Sync messages from the account's IMAP server into the local store.
pub async fn sync_messages(
    account: &Account,
    store: &MailStore,
    reject_invalid_certs: bool,
) -> Result<SyncResult, VivariumError> {
    let mut result = SyncResult::default();

    // Sync inbox
    let r = sync_folder(account, store, "INBOX", "inbox", reject_invalid_certs).await?;
    result.new += r.new;

    // Sync sent
    let sent = account.sent_folder();
    let r = sync_folder(account, store, sent, "sent", reject_invalid_certs).await?;
    result.new += r.new;

    // For Gmail, sync All Mail → archive
    if account.provider == Provider::Gmail {
        let all_mail = account.all_mail_folder();
        let r = sync_folder(account, store, all_mail, "archive", reject_invalid_certs).await?;
        result.new += r.new;
    }

    Ok(result)
}

/// Two-pass sync for a single folder:
/// 1. Fetch UID + size for all messages (fast)
/// 2. Compare against local files
/// 3. Download missing/mismatched messages in parallel chunks
async fn sync_folder(
    account: &Account,
    store: &MailStore,
    remote_folder: &str,
    local_folder: &str,
    reject_invalid_certs: bool,
) -> Result<SyncResult, VivariumError> {
    // Phase 1: SELECT and get message count
    let mut session = connect(account, reject_invalid_certs).await?;
    let mailbox = session
        .select(remote_folder)
        .await
        .map_err(|e| VivariumError::Imap(format!("select {remote_folder} failed: {e}")))?;

    let count = mailbox.exists;
    if count == 0 {
        tracing::info!(folder = remote_folder, "empty folder");
        session.logout().await.ok();
        return Ok(SyncResult::default());
    }

    tracing::info!(folder = remote_folder, count, "checking messages");

    // Phase 2: Fetch all UIDs and sizes
    let fetches = session
        .fetch(format!("1:{count}"), "(UID RFC822.SIZE ENVELOPE)")
        .await
        .map_err(|e| VivariumError::Imap(format!("uid/size fetch failed: {e}")))?;

    let remote_messages: Vec<RemoteMessage> = fetches
        .try_collect::<Vec<_>>()
        .await
        .map_err(|e| VivariumError::Imap(format!("uid/size stream failed: {e}")))?
        .iter()
        .filter_map(|f| {
            let uid = f.uid?;
            let size = u64::from(f.size?);
            let rfc_message_id = f
                .envelope()
                .and_then(|envelope| envelope.message_id.as_deref())
                .and_then(|id| std::str::from_utf8(id).ok())
                .and_then(normalize_message_id);
            Some(RemoteMessage {
                uid,
                size,
                rfc_message_id,
            })
        })
        .collect();

    session.logout().await.ok();

    // Phase 3: Compare against local files
    let local_sizes = store.local_sizes(local_folder)?;
    let rfc_index = store.build_rfc_index(local_folder)?;
    let mut missing: Vec<RemoteMessage> = Vec::new();

    for remote in &remote_messages {
        if let Some(ref rfc_message_id) = remote.rfc_message_id
            && store.rfc_index_lookup(&rfc_index, rfc_message_id, remote.size) {
                continue;
            }

        let msg_id = format!("{local_folder}-{}", remote.uid);
        if let Some(local_size) = local_sizes.get(&msg_id)
            && *local_size == remote.size {
                continue;
            }
        missing.push(remote.clone());
    }

    if missing.is_empty() {
        tracing::info!(
            folder = remote_folder,
            total = remote_messages.len(),
            "all messages up to date"
        );
        return Ok(SyncResult::default());
    }

    tracing::info!(
        folder = remote_folder,
        total = remote_messages.len(),
        missing = missing.len(),
        "downloading new messages"
    );

    // Phase 4: Build UID chunks and download with worker pool
    let chunks: Vec<Vec<RemoteMessage>> = missing
        .chunks(CHUNK_SIZE as usize)
        .map(|c| c.to_vec())
        .collect();
    let chunks = Arc::new(Mutex::new(chunks.into_iter()));
    let result = Arc::new(Mutex::new(SyncResult::default()));
    let store = Arc::new(store.clone());

    let mut handles = Vec::new();
    let worker_count = WORKER_COUNT.min(missing.len().div_ceil(CHUNK_SIZE as usize));

    for worker_id in 0..worker_count {
        let chunks = Arc::clone(&chunks);
        let result = Arc::clone(&result);
        let store = Arc::clone(&store);
        let account = account.clone();
        let local_folder = local_folder.to_string();
        let remote_folder = remote_folder.to_string();

        let handle = tokio::spawn(async move {
            worker(
                WorkerContext {
                    worker_id,
                    account,
                    remote_folder,
                    local_folder,
                    store,
                    reject_invalid_certs,
                },
                &chunks,
                &result,
            )
            .await
        });
        handles.push(handle);
    }

    for handle in handles {
        match handle.await {
            Ok(Ok(())) => {}
            Ok(Err(e)) => return Err(e),
            Err(e) => return Err(VivariumError::Imap(format!("worker join failed: {e}"))),
        }
    }

    let result = Arc::try_unwrap(result).unwrap().into_inner();
    Ok(result)
}

/// Worker: grab chunks from the queue, open a connection, fetch messages.
async fn worker(
    context: WorkerContext,
    chunks: &Mutex<std::vec::IntoIter<Vec<RemoteMessage>>>,
    result: &Mutex<SyncResult>,
) -> Result<(), VivariumError> {
    let mut session = connect(&context.account, context.reject_invalid_certs).await?;
    session
        .select(&context.remote_folder)
        .await
        .map_err(|e| VivariumError::Imap(format!("worker select failed: {e}")))?;

    loop {
        let chunk = {
            let mut iter = chunks.lock().await;
            iter.next()
        };

        let uids = match chunk {
            Some(c) => c,
            None => break,
        };

        let uid_set = uid_set_string(&uids.iter().map(|msg| msg.uid).collect::<Vec<_>>());
        tracing::debug!(
            worker_id = context.worker_id,
            uids = uid_set,
            "fetching chunk"
        );

        let fetches = session
            .uid_fetch(&uid_set, "BODY[]")
            .await
            .map_err(|e| VivariumError::Imap(format!("fetch failed: {e}")))?;

        let messages: Vec<_> = fetches
            .try_collect()
            .await
            .map_err(|e| VivariumError::Imap(format!("fetch stream failed: {e}")))?;

        let mut new_count = 0;
        for fetch in &messages {
            let uid = fetch.uid.unwrap_or(0);
            let body = match fetch.body() {
                Some(b) => b,
                None => continue,
            };

            let message_id = format!("{}-{uid}", context.local_folder);
            let subdir = if context.local_folder == "inbox" {
                "new"
            } else {
                "cur"
            };
            context
                .store
                .store_message_in(&context.local_folder, subdir, &message_id, body)?;
            if let Some(rfc_message_id) = message_id_from_bytes(body) {
                let size = u64::try_from(body.len()).unwrap_or(u64::MAX);
                context.store.write_message_index(
                    &context.local_folder,
                    &rfc_message_id,
                    uid,
                    size,
                )?;
            }
            new_count += 1;
        }

        if new_count > 0 {
            let mut r = result.lock().await;
            r.new += new_count;
        }
    }

    session.logout().await.ok();
    Ok(())
}

/// Build a UID set string like "1,2,3,5,6,7" from a list of UIDs.
fn uid_set_string(uids: &[u32]) -> String {
    uids.iter()
        .map(|u| u.to_string())
        .collect::<Vec<_>>()
        .join(",")
}

fn xoauth2_initial_response(user: &str, access_token: &str) -> String {
    format!("user={user}\x01auth=Bearer {access_token}\x01\x01")
}

pub async fn idle(account: &Account, reject_invalid_certs: bool) -> Result<(), VivariumError> {
    let mut session = connect(account, reject_invalid_certs).await?;
    session
        .select("INBOX")
        .await
        .map_err(|e| VivariumError::Imap(format!("idle select INBOX failed: {e}")))?;

    let mut handle = session.idle();
    handle
        .init()
        .await
        .map_err(|e| VivariumError::Imap(format!("IDLE init failed: {e}")))?;

    let (wait, _interrupt) = handle.wait_with_timeout(Duration::from_secs(29 * 60));
    match wait
        .await
        .map_err(|e| VivariumError::Imap(format!("IDLE wait failed: {e}")))?
    {
        IdleResponse::NewData(response) => {
            tracing::debug!(response = ?response.parsed(), "IMAP IDLE notification");
        }
        IdleResponse::Timeout => {
            tracing::debug!("IMAP IDLE timed out; refreshing connection");
        }
        IdleResponse::ManualInterrupt => {
            tracing::debug!("IMAP IDLE interrupted");
        }
    }

    let mut session = handle
        .done()
        .await
        .map_err(|e| VivariumError::Imap(format!("IDLE done failed: {e}")))?;
    session.logout().await.ok();
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_xoauth2_initial_response() {
        assert_eq!(
            xoauth2_initial_response("me@example.com", "token"),
            "user=me@example.com\u{1}auth=Bearer token\u{1}\u{1}"
        );
    }
}

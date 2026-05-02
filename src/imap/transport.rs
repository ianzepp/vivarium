use async_imap::{Authenticator, Session};
use std::fmt;
use tokio::net::TcpStream;
use tokio_native_tls::TlsStream;

use crate::config::{Account, Auth, Provider, Security};
use crate::error::VivariumError;

pub(super) type ImapSession = Session<TlsStream<TcpStream>>;

pub(super) const CHUNK_SIZE: u32 = 100;
pub(super) const WORKER_COUNT: usize = 4;

#[derive(Debug, Clone)]
pub(super) struct RemoteMessage {
    pub(super) uid: u32,
    pub(super) uidvalidity: Option<u32>,
    pub(super) size: u64,
    pub(super) rfc_message_id: Option<String>,
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

fn xoauth2_initial_response(user: &str, access_token: &str) -> String {
    format!("user={user}\x01auth=Bearer {access_token}\x01\x01")
}

/// Connect and authenticate to the account's IMAP server.
pub async fn connect(
    account: &Account,
    reject_invalid_certs: bool,
) -> Result<ImapSession, VivariumError> {
    let host = account.resolved_imap_host();
    let port = account.resolved_imap_port();
    let secret = account.resolve_secret().await?;

    tracing::debug!(host, port, security = %account.imap_security, "connecting to IMAP");

    let tcp = TcpStream::connect((host.as_str(), port))
        .await
        .map_err(|e| {
            if account.provider == Provider::Protonmail {
                VivariumError::Imap(format!(
                    "cannot reach Proton Bridge at {host}:{port} (is Bridge running?). \
                     Details: {e}"
                ))
            } else {
                VivariumError::Imap(format!("TCP connect to {host}:{port} failed: {e}"))
            }
        })?;

    let tls_connector = build_tls_connector(reject_invalid_certs)?;
    let tls_stream = tls_stream(&tls_connector, account, tcp).await;
    match tls_stream {
        Ok(session) => {
            let authenticated = authenticate(account, secret, session).await;
            tracing::debug!(account = account.name, "IMAP authenticated");
            authenticated
        }
        Err(e) => {
            if account.provider == Provider::Protonmail {
                return Err(VivariumError::Imap(format!(
                    "TLS error connecting to Proton Bridge at {host}:{port}. \
                     If using a self-signed certificate, set reject_invalid_certs = true \
                     in config.toml or add the cert to your keychain. Details: {e}"
                )));
            }
            Err(e)
        }
    }
}

fn build_tls_connector(
    reject_invalid_certs: bool,
) -> Result<tokio_native_tls::TlsConnector, VivariumError> {
    let mut tls_builder = native_tls::TlsConnector::builder();
    if !reject_invalid_certs {
        tls_builder.danger_accept_invalid_certs(true);
    }
    let native_connector = tls_builder
        .build()
        .map_err(|e| VivariumError::Tls(format!("TLS connector build failed: {e}")))?;
    Ok(tokio_native_tls::TlsConnector::from(native_connector))
}

async fn tls_stream(
    tls_connector: &tokio_native_tls::TlsConnector,
    account: &Account,
    tcp: TcpStream,
) -> Result<TlsStream<TcpStream>, VivariumError> {
    let host = &account.imap_host;
    match account.imap_security {
        Security::Ssl => tls_connector
            .connect(host, tcp)
            .await
            .map_err(|e| VivariumError::Tls(format!("TLS handshake failed: {e}"))),
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
                .map_err(|e| VivariumError::Tls(format!("STARTTLS TLS upgrade failed: {e}")))
        }
    }
}

async fn authenticate(
    account: &Account,
    secret: String,
    tls_stream: TlsStream<TcpStream>,
) -> Result<ImapSession, VivariumError> {
    let client = async_imap::Client::new(tls_stream);
    match account.auth {
        Auth::Password => client
            .login(&account.username, &secret)
            .await
            .map_err(|(e, _)| VivariumError::Imap(format!("login failed: {e}"))),
        Auth::Xoauth2 => client
            .authenticate(
                "XOAUTH2",
                Xoauth2 {
                    user: account.username.clone(),
                    access_token: secret,
                },
            )
            .await
            .map_err(|(e, _)| VivariumError::Imap(format!("XOAUTH2 failed: {e}"))),
    }
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

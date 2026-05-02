use lettre::address::{Address, Envelope};
use lettre::transport::smtp::authentication::{Credentials, Mechanism};
use lettre::{AsyncSmtpTransport, AsyncTransport, Tokio1Executor};

use crate::config::{Account, Auth, Security};
use crate::error::VivariumError;

/// Send raw .eml bytes via the account's SMTP server.
#[cfg(feature = "outbox")]
pub async fn send_raw(
    account: &Account,
    data: &[u8],
    reject_invalid_certs: bool,
) -> Result<(), VivariumError> {
    let host = account.resolved_smtp_host();
    let port = account.resolved_smtp_port();
    let secret = account.resolve_secret().await?;

    tracing::info!(host, port, security = %account.smtp_security, "connecting to SMTP");

    let creds = Credentials::new(account.username.clone(), secret);

    let tls_parameters = tls_parameters(host, reject_invalid_certs)?;

    let builder = match account.smtp_security {
        Security::Ssl => AsyncSmtpTransport::<Tokio1Executor>::builder_dangerous(host)
            .port(port)
            .tls(lettre::transport::smtp::client::Tls::Wrapper(
                tls_parameters,
            )),
        Security::Starttls => AsyncSmtpTransport::<Tokio1Executor>::builder_dangerous(host)
            .port(port)
            .tls(lettre::transport::smtp::client::Tls::Required(
                tls_parameters,
            )),
    };

    let builder = match account.auth {
        Auth::Password => builder,
        Auth::Xoauth2 => builder.authentication(vec![Mechanism::Xoauth2]),
    };
    let transport = builder.credentials(creds).build();
    let envelope = envelope_from_raw(data)?;

    transport
        .send_raw(&envelope, data)
        .await
        .map_err(|e| VivariumError::Smtp(format!("send failed: {e}")))?;

    tracing::info!("message sent");
    Ok(())
}

fn tls_parameters(
    host: &str,
    reject_invalid_certs: bool,
) -> Result<lettre::transport::smtp::client::TlsParameters, VivariumError> {
    if reject_invalid_certs {
        return lettre::transport::smtp::client::TlsParameters::builder(host.to_string())
            .build()
            .map_err(|e| VivariumError::Tls(format!("TLS params failed: {e}")));
    }

    lettre::transport::smtp::client::TlsParameters::builder(host.to_string())
        .dangerous_accept_invalid_certs(true)
        .build()
        .map_err(|e| VivariumError::Tls(format!("TLS params failed: {e}")))
}

/// Extract From/To addresses from raw .eml to build a lettre Envelope.
fn envelope_from_raw(data: &[u8]) -> Result<Envelope, VivariumError> {
    let parsed = mail_parser::MessageParser::default()
        .parse(data)
        .ok_or_else(|| VivariumError::Smtp("failed to parse message for envelope".into()))?;

    let from_addr = parsed
        .from()
        .and_then(|a| a.first())
        .and_then(|a| a.address())
        .ok_or_else(|| VivariumError::Smtp("message has no From address".into()))?;

    let from: Address = from_addr
        .parse()
        .map_err(|e| VivariumError::Smtp(format!("invalid From address: {e}")))?;

    let mut to_addrs = Vec::new();
    if let Some(to_list) = parsed.to() {
        for addr in to_list.iter() {
            if let Some(email) = addr.address() {
                let a: Address = email
                    .parse()
                    .map_err(|e| VivariumError::Smtp(format!("invalid To address: {e}")))?;
                to_addrs.push(a);
            }
        }
    }

    if to_addrs.is_empty() {
        return Err(VivariumError::Smtp("message has no To addresses".into()));
    }

    Envelope::new(Some(from), to_addrs)
        .map_err(|e| VivariumError::Smtp(format!("envelope error: {e}")))
}

use lettre::address::{Address, Envelope};
use lettre::transport::smtp::authentication::{Credentials, Mechanism};
use lettre::{AsyncSmtpTransport, AsyncTransport, Tokio1Executor};

use crate::config::{Account, Auth, Security};
use crate::error::VivariumError;

/// Send raw .eml bytes via the account's SMTP server.
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

    let tls_parameters = tls_parameters(&host, reject_invalid_certs)?;

    let builder = match account.smtp_security {
        Security::Ssl => AsyncSmtpTransport::<Tokio1Executor>::builder_dangerous(host.clone())
            .port(port)
            .tls(lettre::transport::smtp::client::Tls::Wrapper(
                tls_parameters,
            )),
        Security::Starttls => AsyncSmtpTransport::<Tokio1Executor>::builder_dangerous(host.clone())
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

    transport.send_raw(&envelope, data).await.map_err(|e| {
        VivariumError::Smtp(format!(
            "send failed via {host}:{port} using {}: {e}",
            account.smtp_security
        ))
    })?;

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
    collect_addresses("To", parsed.to(), &mut to_addrs)?;
    collect_addresses("Cc", parsed.cc(), &mut to_addrs)?;
    collect_addresses("Bcc", parsed.bcc(), &mut to_addrs)?;

    if to_addrs.is_empty() {
        return Err(VivariumError::Smtp("message has no recipients".into()));
    }

    Envelope::new(Some(from), to_addrs)
        .map_err(|e| VivariumError::Smtp(format!("envelope error: {e}")))
}

fn collect_addresses(
    label: &str,
    list: Option<&mail_parser::Address<'_>>,
    out: &mut Vec<Address>,
) -> Result<(), VivariumError> {
    if let Some(list) = list {
        for addr in list.iter() {
            if let Some(email) = addr.address() {
                let parsed: Address = email
                    .parse()
                    .map_err(|e| VivariumError::Smtp(format!("invalid {label} address: {e}")))?;
                out.push(parsed);
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn envelope_includes_to_cc_and_bcc_recipients() {
        let data = b"From: sender@example.com\r\nTo: a@example.com\r\nCc: b@example.com\r\nBcc: c@example.com\r\nSubject: hi\r\n\r\nbody";

        let envelope = envelope_from_raw(data).unwrap();
        let recipients = envelope
            .to()
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>();

        assert_eq!(
            recipients,
            vec!["a@example.com", "b@example.com", "c@example.com"]
        );
    }

    #[test]
    fn envelope_requires_at_least_one_recipient() {
        let data = b"From: sender@example.com\r\nSubject: hi\r\n\r\nbody";

        let err = envelope_from_raw(data).unwrap_err();

        assert!(err.to_string().contains("no recipients"));
    }
}

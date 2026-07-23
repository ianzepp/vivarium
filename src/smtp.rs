use lettre::address::{Address, Envelope};
use lettre::transport::smtp::authentication::{Credentials, Mechanism};
use lettre::{AsyncSmtpTransport, AsyncTransport, Tokio1Executor};

use crate::config::{Account, Auth, Security};
use crate::error::VivariumError;

/// Send raw .eml bytes via the account's SMTP server.
///
/// Bcc headers are stripped from the transmitted DATA (RFC 5322 §3.6.3)
/// while remaining in the SMTP envelope so delivery reaches all recipients.
///
/// # Errors
/// Returns an error if the SMTP transport cannot be built, sending fails,
/// or the message is malformed.
pub async fn send_raw(
    account: &Account,
    data: &[u8],
    reject_invalid_certs: bool,
) -> Result<(), VivariumError> {
    let secret = account.resolve_secret().await?;
    let transport = smtp_transport(account, secret, reject_invalid_certs)?;

    let (envelope, sanitized) = prepare_for_send(data)?;

    transport
        .send_raw(&envelope, &sanitized)
        .await
        .map_err(|e| {
            VivariumError::Smtp(format!(
                "send failed via {}:{} using {}: {e}",
                account.resolved_smtp_host(),
                account.resolved_smtp_port(),
                account.resolved_smtp_security()
            ))
        })?;

    tracing::info!("message sent");
    Ok(())
}

/// Build the SMTP envelope (including Bcc recipients for delivery) and
/// sanitize the DATA bytes (stripping Bcc headers for privacy).
///
/// Both production `send_raw` and tests call this seam to exercise the
/// same envelope-parsing and Bcc-sanitization code path.
fn prepare_for_send(data: &[u8]) -> Result<(Envelope, Vec<u8>), VivariumError> {
    let envelope = envelope_from_raw(data)?;
    let sanitized = strip_bcc_headers(data);
    Ok((envelope, sanitized))
}

/// Test SMTP connectivity, TLS, auth, and NOOP without sending mail.
///
/// # Errors
/// Returns an error if the SMTP transport cannot be built or the connection
/// test fails.
pub async fn test_connection(
    account: &Account,
    reject_invalid_certs: bool,
) -> Result<bool, VivariumError> {
    let secret = account.resolve_secret().await?;
    let transport = smtp_transport(account, secret, reject_invalid_certs)?;
    transport
        .test_connection()
        .await
        .map_err(|e| VivariumError::Smtp(format!("SMTP connection test failed: {e}")))
}

fn smtp_transport(
    account: &Account,
    secret: String,
    reject_invalid_certs: bool,
) -> Result<AsyncSmtpTransport<Tokio1Executor>, VivariumError> {
    let host = account.resolved_smtp_host();
    let port = account.resolved_smtp_port();
    let security = account.resolved_smtp_security();
    tracing::info!(host, port, security = %security, "connecting to SMTP");

    let creds = Credentials::new(account.username.clone(), secret);

    let tls_parameters = tls_parameters(&host, reject_invalid_certs)?;

    let builder = match security {
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
    Ok(builder.credentials(creds).build())
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
        .dangerous_accept_invalid_hostnames(true)
        .build()
        .map_err(|e| VivariumError::Tls(format!("TLS params failed: {e}")))
}

/// Strip all Bcc headers (case-insensitive, including folded continuations)
/// from the raw message bytes, preserving everything else.
///
/// Per RFC 5322 §3.6.3, Bcc recipients must not appear in the DATA sent
/// to the SMTP server. The envelope (RCPT TO) still includes them.
fn strip_bcc_headers(data: &[u8]) -> Vec<u8> {
    let Ok(text) = std::str::from_utf8(data) else {
        return data.to_vec();
    };
    let Some((header_block, body)) = text.split_once("\r\n\r\n") else {
        return data.to_vec();
    };
    let mut result = String::new();
    let mut in_bcc = false;
    for line in header_block.split_inclusive("\r\n") {
        let trimmed = line.trim_end_matches("\r\n");
        if is_bcc_header(trimmed) {
            in_bcc = true;
            continue;
        }
        if in_bcc && is_folded_continuation(trimmed) {
            continue;
        }
        in_bcc = false;
        result.push_str(line);
    }
    result.push_str("\r\n\r\n");
    result.push_str(body);
    result.into_bytes()
}

fn is_bcc_header(line: &str) -> bool {
    let lower = line.to_ascii_lowercase();
    lower.starts_with("bcc:")
}

fn is_folded_continuation(line: &str) -> bool {
    line.starts_with(' ') || line.starts_with('\t')
}
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
#[path = "smtp_test.rs"]
mod tests;

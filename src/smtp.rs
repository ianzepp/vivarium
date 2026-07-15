use lettre::address::{Address, Envelope};
use lettre::transport::smtp::authentication::{Credentials, Mechanism};
use lettre::{AsyncSmtpTransport, AsyncTransport, Tokio1Executor};

use crate::config::{Account, Auth, Security};
use crate::error::VivariumError;

/// Send raw .eml bytes via the account's SMTP server.
///
/// Bcc headers are stripped from the transmitted DATA (RFC 5322 §3.6.3)
/// while remaining in the SMTP envelope so delivery reaches all recipients.
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
    let text = match std::str::from_utf8(data) {
        Ok(s) => s,
        Err(_) => return data.to_vec(),
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

    #[test]
    fn smtp_transport_builds_for_supported_security_and_auth_modes() {
        let mut account = test_account();
        account.smtp_security = Some(Security::Ssl);
        account.auth = Auth::Password;
        smtp_transport(&account, "secret".into(), true).unwrap();

        account.smtp_security = Some(Security::Starttls);
        account.auth = Auth::Xoauth2;
        smtp_transport(&account, "access-token".into(), true).unwrap();
    }

    #[test]
    fn strip_bcc_removes_simple_bcc_header() {
        let data = b"From: a@example.com\r\nTo: b@example.com\r\nBcc: c@example.com\r\nSubject: hi\r\n\r\nbody";
        let result = strip_bcc_headers(data);
        let text = std::str::from_utf8(&result).unwrap();
        assert!(!text.to_ascii_lowercase().contains("bcc:"));
        assert!(text.contains("From: a@example.com"));
        assert!(text.contains("To: b@example.com"));
        assert!(text.contains("Subject: hi"));
        assert!(text.contains("\r\n\r\nbody"));
    }

    #[test]
    fn strip_bcc_removes_case_insensitive_bcc() {
        let data = b"From: a@example.com\r\nBCC: c@example.com\r\nbcc: d@example.com\r\nTo: b@example.com\r\n\r\nbody";
        let result = strip_bcc_headers(data);
        let text = std::str::from_utf8(&result).unwrap();
        assert!(!text.to_ascii_lowercase().contains("bcc"));
    }

    #[test]
    fn strip_bcc_removes_folded_continuation_lines() {
        let data = b"From: a@example.com\r\nTo: b@example.com\r\nBcc: c@example.com,\r\n d@example.com\r\nSubject: hi\r\n\r\nbody";
        let result = strip_bcc_headers(data);
        let text = std::str::from_utf8(&result).unwrap();
        assert!(!text.to_ascii_lowercase().contains("bcc"));
        assert!(!text.contains("d@example.com"));
        assert!(text.contains("Subject: hi"));
        assert!(text.contains("\r\n\r\nbody"));
    }

    #[test]
    fn strip_bcc_preserves_message_with_no_bcc() {
        let data = b"From: a@example.com\r\nTo: b@example.com\r\nSubject: hi\r\n\r\nbody";
        let result = strip_bcc_headers(data);
        assert_eq!(result, data);
    }

    #[test]
    fn strip_bcc_preserves_body_intact() {
        let data = b"From: a@example.com\r\nTo: b@example.com\r\nBcc: c@example.com\r\n\r\nLine one.\r\nBcc: should not be stripped in body\r\nLine three.";
        let result = strip_bcc_headers(data);
        let text = std::str::from_utf8(&result).unwrap();
        assert!(
            !text[..text.find("\r\n\r\n").unwrap()]
                .to_ascii_lowercase()
                .contains("bcc")
        );
        assert!(text.contains("Bcc: should not be stripped in body"));
    }

    #[test]
    fn fake_smtp_capture_envelope_has_bcc_data_does_not() {
        let data = b"From: sender@example.com\r\nTo: to@example.com\r\nCc: cc@example.com\r\nBcc: secret@example.com\r\nSubject: test\r\n\r\nbody";

        // Envelope must include all recipients (To + Cc + Bcc).
        let envelope = envelope_from_raw(data).unwrap();
        let recipients = envelope
            .to()
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>();
        assert!(recipients.contains(&"secret@example.com".into()));

        // Captured DATA must not contain any Bcc header.
        let captured = strip_bcc_headers(data);
        let captured_text = std::str::from_utf8(&captured).unwrap();
        let header_block = captured_text
            .split_once("\r\n\r\n")
            .map(|(h, _)| h)
            .unwrap_or(captured_text);
        assert!(
            !header_block.to_ascii_lowercase().contains("bcc"),
            "DATA header block must not contain Bcc: {header_block}"
        );

        // Captured DATA still has To and Cc for transparency.
        assert!(captured_text.contains("To: to@example.com"));
        assert!(captured_text.contains("Cc: cc@example.com"));
    }

    #[tokio::test]
    async fn send_seam_strips_bcc_from_captured_data() {
        use lettre::transport::stub::AsyncStubTransport;

        let data = b"From: sender@example.com\r\nTo: to@example.com\r\nCc: cc@example.com\r\nBcc: hidden@example.com\r\nSubject: private\r\n\r\nbody";

        // Exercise the same prepare_for_send seam that send_raw uses.
        let (envelope, sanitized) = prepare_for_send(data).unwrap();

        // Feed the exact values to a fake transport that captures them.
        let stub = AsyncStubTransport::new_ok();
        stub.send_raw(&envelope, &sanitized).await.unwrap();

        let messages = stub.messages().await;
        assert_eq!(messages.len(), 1);
        let (captured_envelope, captured_data) = &messages[0];

        // Envelope must include Bcc recipient for delivery.
        let recipients: Vec<String> = captured_envelope
            .to()
            .iter()
            .map(ToString::to_string)
            .collect();
        assert!(recipients.contains(&"hidden@example.com".into()));
        assert!(recipients.contains(&"to@example.com".into()));
        assert!(recipients.contains(&"cc@example.com".into()));

        // Captured DATA must not contain any Bcc header.
        let header_block = captured_data
            .split_once("\r\n\r\n")
            .map(|(h, _)| h)
            .unwrap_or(captured_data);
        assert!(
            !header_block.to_ascii_lowercase().contains("bcc"),
            "DATA must not contain Bcc: {header_block}"
        );

        // Original raw bytes retain Bcc for local reconciliation.
        let original = std::str::from_utf8(data).unwrap();
        assert!(original.contains("Bcc: hidden@example.com"));
    }

    #[tokio::test]
    async fn send_seam_preserves_message_without_bcc() {
        use lettre::transport::stub::AsyncStubTransport;

        let data = b"From: sender@example.com\r\nTo: to@example.com\r\nSubject: clean\r\n\r\nbody";

        let (envelope, sanitized) = prepare_for_send(data).unwrap();

        let stub = AsyncStubTransport::new_ok();
        stub.send_raw(&envelope, &sanitized).await.unwrap();

        let messages = stub.messages().await;
        assert_eq!(messages.len(), 1);
        let (_, captured_data) = &messages[0];
        assert_eq!(captured_data.as_bytes(), data);
    }

    fn test_account() -> Account {
        Account {
            name: "smtp-test".into(),
            email: "sender@example.com".into(),
            imap_host: "imap.example.com".into(),
            imap_port: Some(993),
            imap_security: Some(Security::Ssl),
            smtp_host: "smtp.example.com".into(),
            smtp_port: Some(587),
            smtp_security: Some(Security::Starttls),
            username: "sender@example.com".into(),
            auth: Auth::Password,
            password: Some("secret".into()),
            password_cmd: None,
            token_cmd: None,
            oauth_client_id: None,
            oauth_client_secret: None,
            mail_dir: None,
            inbox_folder: None,
            archive_folder: None,
            trash_folder: None,
            sent_folder: None,
            drafts_folder: None,
            label_roots: None,
            storage_mode: None,
            provider: crate::config::Provider::Standard,
            oauth_authorization_url: None,
            oauth_token_url: None,
            oauth_scope: None,
            reject_invalid_certs: None,
            policy: crate::config::MutationPolicy::FullWrite,
        }
    }
}

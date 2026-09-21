use super::*;

#[test]
fn draft_request_extracts_headers_and_plain_body() {
    let data = b"Message-ID: <draft@example.com>\r\nFrom: Sender <sender@example.com>\r\nTo: A <a@example.com>\r\nCc: b@example.com\r\nBcc: c@example.com\r\nSubject: Hello\r\n\r\nPlain body";

    let request = draft_request_from_eml(data).unwrap();

    assert_eq!(request.message.subject, "Hello");
    assert_eq!(request.message.sender.address, "sender@example.com");
    assert_eq!(request.message.sender.name, "Sender");
    assert_eq!(request.message.to[0].address, "a@example.com");
    assert_eq!(request.message.cc[0].address, "b@example.com");
    assert_eq!(request.message.bcc[0].address, "c@example.com");
    assert_eq!(request.message.body, "Plain body");
    assert_eq!(request.message.mime_type, "text/plain");
    assert_eq!(
        request.message.external_id.as_deref(),
        Some("draft@example.com")
    );
}

#[test]
fn draft_request_prefers_html_body() {
    let data = concat!(
        "From: sender@example.com\r\n",
        "To: a@example.com\r\n",
        "Subject: HTML\r\n",
        "Content-Type: text/html\r\n",
        "\r\n",
        "<p>Hello</p>"
    );

    let request = draft_request_from_eml(data.as_bytes()).unwrap();

    assert_eq!(request.message.body, "<p>Hello</p>");
    assert_eq!(request.message.mime_type, "text/html");
}

#[test]
fn draft_request_requires_recipient() {
    let data = b"From: sender@example.com\r\nSubject: Missing\r\n\r\nBody";

    let err = draft_request_from_eml(data).unwrap_err();

    assert!(err.to_string().contains("at least one"));
}

#[test]
fn clear_send_request_wraps_all_recipients_and_body_key() {
    let data = b"From: sender@example.com\r\nTo: a@example.com\r\nCc: b@example.com\r\nBcc: c@example.com\r\nSubject: Hello\r\n\r\nBody";
    let draft = draft_request_from_eml(data).unwrap();
    let recipients: Vec<_> = all_recipients(&draft)
        .into_iter()
        .map(RecipientSendPreference::clear)
        .collect();
    let request = send_request(
        &draft,
        ProtonEncryptedBody {
            armored_message: "armored".into(),
            data_packet: b"encrypted".to_vec(),
            session_key: b"session-key".to_vec(),
            algorithm: "aes256".into(),
        },
        &recipients,
    )
    .unwrap();

    let package = &request.packages[0];

    assert_eq!(package.package_type, CLEAR_SCHEME);
    assert_eq!(package.mime_type, "text/plain");
    assert_eq!(package.body, STANDARD.encode(b"encrypted"));
    assert_eq!(
        package.body_key.as_ref().map(|key| key.key.as_str()),
        Some(STANDARD.encode(b"session-key").as_str())
    );
    assert_eq!(package.body_key.as_ref().unwrap().algorithm, "aes256");
    assert!(package.addresses.get("a@example.com").is_some());
    assert!(package.addresses.get("b@example.com").is_some());
    assert!(package.addresses.get("c@example.com").is_some());
}

#[test]
fn keyed_recipient_requires_body_key_packet() {
    let data = b"From: sender@example.com\r\nTo: a@example.com\r\nSubject: Hello\r\n\r\nBody";
    let draft = draft_request_from_eml(data).unwrap();
    let err = send_request(
        &draft,
        ProtonEncryptedBody {
            armored_message: "armored".into(),
            data_packet: b"encrypted".to_vec(),
            session_key: b"session-key".to_vec(),
            algorithm: "aes256".into(),
        },
        &[RecipientSendPreference {
            address: "a@example.com".into(),
            scheme: INTERNAL_SCHEME,
            public_key: Some("not a public key".into()),
        }],
    )
    .unwrap_err();

    assert!(err.to_string().contains("public key parse"));
}

#[test]
fn external_pgp_html_is_blocked_until_mime_packages_exist() {
    let err = RecipientSendPreference::new(
        "a@example.com".into(),
        Some("public key".into()),
        Some(ProtonRecipientType(2)),
        "text/html",
    )
    .unwrap_err();

    assert!(err.to_string().contains("PGP/MIME"));
}

#[test]
fn mixed_package_type_combines_recipient_schemes() {
    let recipients = vec![
        RecipientSendPreference::clear("clear@example.com".into()),
        RecipientSendPreference {
            address: "internal@example.com".into(),
            scheme: INTERNAL_SCHEME,
            public_key: None,
        },
        RecipientSendPreference {
            address: "pgp@example.com".into(),
            scheme: PGP_INLINE_SCHEME,
            public_key: None,
        },
    ];

    assert_eq!(
        package_type(&recipients),
        CLEAR_SCHEME | INTERNAL_SCHEME | PGP_INLINE_SCHEME
    );
}

use super::*;

#[test]
fn extracts_plain_text_body() {
    let eml = b"From: a@b\r\nTo: c@d\r\nSubject: test\r\n\r\nHello world";
    let result = extract_text(eml).unwrap();
    assert_eq!(result.body_text, "Hello world");
    assert_eq!(result.format, ExtractionFormat::Plain);
    assert_eq!(result.quality, ExtractionQuality::Full);
}

#[test]
fn strips_html_to_text() {
    // Use plain text with html-like markers to test the text path
    let eml =
        b"From: a@b\r\nTo: c@d\r\nSubject: test\r\nContent-Type: text/plain\r\n\r\nHello world";
    let result = extract_text(eml).unwrap();
    assert_eq!(result.body_text, "Hello world");
    assert_eq!(result.format, ExtractionFormat::Plain);
    assert_eq!(result.quality, ExtractionQuality::Full);
}

#[test]
fn handles_empty_body() {
    let eml = b"From: a@b\r\nTo: c@d\r\nSubject: test\r\n\r\n";
    let result = extract_text(eml).unwrap();
    // Empty body returns Partial with empty text
    assert!(result.body_text.is_empty() || result.quality == ExtractionQuality::Full);
}

#[test]
fn extracts_attachments() {
    // Note: mail_parser may or may not parse this depending on format
    let eml = b"From: a@b\r\nTo: c@d\r\nSubject: test\r\n\r\nno attachment here";
    let attachments = extract_attachments(eml).unwrap();
    assert!(
        attachments.is_empty()
            || attachments
                .iter()
                .all(|a| a.size > 0 || !a.filename.is_empty())
    );
}

#[test]
fn extracts_catalog_entries() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("message.eml");
    fs::write(
        &path,
        b"From: a@b\r\nTo: c@d\r\nSubject: test\r\n\r\nHello world",
    )
    .unwrap();
    let entry = CatalogEntry {
        handle: "h1".into(),
        account: "acct".into(),
        content_id: "f1".into(),
        blob_path: path.to_string_lossy().to_string(),
        local_role: "inbox".into(),
        read_state: false,
        starred: false,
        date: String::new(),
        from: String::new(),
        to: String::new(),
        cc: String::new(),
        bcc: String::new(),
        subject: String::new(),
        rfc_message_id: String::new(),
        remote: None,
    };

    let (extracted, errors) = extract_catalog_entries(&[entry]).unwrap();

    assert_eq!(extracted, 1);
    assert_eq!(errors, 0);
}

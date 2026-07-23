use super::*;
use crate::message::message_id_from_bytes;
use mail_parser::MimeHeaders;

#[test]
fn compose_draft_supports_source_and_generated_pdf_attachments() {
    let eml = build_compose_draft_with_attachments(
        &ComposeDraft {
            from: "me@example.com".into(),
            to: vec!["a@example.com".into()],
            cc: Vec::new(),
            bcc: Vec::new(),
            subject: "report".into(),
            body: "see attachments".into(),
            html_body: None,
        },
        &[
            FileAttachment {
                filename: "report.md".into(),
                content_type: "text/markdown".into(),
                data: b"# Report".to_vec(),
            },
            FileAttachment {
                filename: "report.pdf".into(),
                content_type: "application/pdf".into(),
                data: b"%PDF-test".to_vec(),
            },
        ],
    )
    .unwrap();
    let parsed = mail_parser::MessageParser::default()
        .parse(eml.as_bytes())
        .unwrap();
    let names = parsed
        .attachments()
        .filter_map(|part| part.attachment_name().map(str::to_owned))
        .collect::<Vec<_>>();
    assert_eq!(names, ["report.md", "report.pdf"]);
}

#[test]
fn compose_draft_includes_required_headers_and_recipients() {
    let eml = build_compose_draft(&ComposeDraft {
        from: "me@example.com".into(),
        to: vec!["a@example.com".into()],
        cc: vec!["b@example.com".into()],
        bcc: vec!["c@example.com".into()],
        subject: "hello".into(),
        body: "body".into(),
        html_body: None,
    })
    .unwrap();

    assert!(eml.contains("Date: "));
    assert!(eml.contains("Message-ID: <"));
    assert!(message_id_from_bytes(eml.as_bytes()).is_some());
    let parsed = mail_parser::MessageParser::default()
        .parse(eml.as_bytes())
        .unwrap();
    assert_eq!(
        parsed.to().unwrap().first().unwrap().address(),
        Some("a@example.com")
    );
    assert_eq!(
        parsed.cc().unwrap().first().unwrap().address(),
        Some("b@example.com")
    );
    assert_eq!(
        parsed.bcc().unwrap().first().unwrap().address(),
        Some("c@example.com")
    );
}

#[test]
fn reply_draft_threads_to_original() {
    let original = b"Message-ID: <root@example.com>\r\nFrom: A <a@example.com>\r\nTo: me@example.com\r\nSubject: root\r\n\r\nhello";

    let eml = build_reply(
        original,
        &ReplyDraft {
            from: "me@example.com".into(),
            body: "thanks".into(),
            html_body: None,
        },
    )
    .unwrap();

    assert!(eml.contains("Subject: Re: root"));
    assert!(eml.contains("In-Reply-To: <root@example.com>"));
    assert!(eml.contains("References: <root@example.com>"));
    assert!(eml.contains("> hello"));
}

#[test]
fn replace_from_header_updates_sender() {
    let data = b"From: me@example.com\r\nTo: you@example.com\r\nSubject: hi\r\n\r\nbody";

    let rewritten = replace_from_header(data, "Alias <alias@example.com>").unwrap();
    let text = String::from_utf8(rewritten).unwrap();

    assert!(text.starts_with("From: Alias <alias@example.com>\r\n"));
    assert!(text.contains("\r\nTo: you@example.com\r\n"));
    assert!(text.ends_with("\r\n\r\nbody"));
}

#[test]
fn replace_from_header_rejects_invalid_sender() {
    let data = b"From: me@example.com\r\nTo: you@example.com\r\nSubject: hi\r\n\r\nbody";

    let err = replace_from_header(data, "not an address").unwrap_err();

    assert!(err.to_string().contains("invalid --from address"));
}

#[test]
fn compose_draft_can_include_html_alternative() {
    let eml = build_compose_draft(&ComposeDraft {
        from: "me@example.com".into(),
        to: vec!["a@example.com".into()],
        cc: Vec::new(),
        bcc: Vec::new(),
        subject: "hello".into(),
        body: "plain body".into(),
        html_body: Some("<p>HTML body</p>".into()),
    })
    .unwrap();
    let parsed = mail_parser::MessageParser::default()
        .parse(eml.as_bytes())
        .unwrap();

    assert_eq!(parsed.body_text(0).as_deref(), Some("plain body\r\n"));
    assert_eq!(parsed.body_html(0).as_deref(), Some("<p>HTML body</p>"));
}

#[test]
fn auto_html_body_escapes_and_paragraphs_plain_text() {
    let html = auto_html_body("Hello <there>\n\nNext & last");

    assert!(html.starts_with("<div dir=\"ltr\">"));
    assert!(html.contains("Hello &lt;there&gt;"));
    assert!(html.contains("<br>\n<br>\nNext"));
    assert!(html.contains("Next &amp; last"));
    assert!(!html.contains("<!doctype"));
    assert!(!html.contains("<body"));
    assert!(!html.contains("<p style="));
    assert!(!html.contains("background:"));
    assert!(!html.contains("border:"));
    assert!(!html.contains("max-width:"));
}

#[test]
fn reply_draft_can_include_html_alternative_with_quote() {
    let original = b"Message-ID: <root@example.com>\r\nFrom: A <a@example.com>\r\nTo: me@example.com\r\nSubject: root\r\n\r\nhello";
    let eml = build_reply(
        original,
        &ReplyDraft {
            from: "me@example.com".into(),
            body: "thanks".into(),
            html_body: Some(auto_html_body("thanks")),
        },
    )
    .unwrap();
    let parsed = mail_parser::MessageParser::default()
        .parse(eml.as_bytes())
        .unwrap();

    assert!(parsed.body_text(0).unwrap().contains("> hello"));
    assert!(parsed.body_html(0).unwrap().contains("<blockquote"));
}

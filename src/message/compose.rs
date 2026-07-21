use crate::error::VivariumError;

#[derive(Debug, Clone)]
pub struct ComposeDraft {
    pub from: String,
    pub to: Vec<String>,
    pub cc: Vec<String>,
    pub bcc: Vec<String>,
    pub subject: String,
    pub body: String,
    pub html_body: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ReplyDraft {
    pub from: String,
    pub body: String,
    pub html_body: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileAttachment {
    pub filename: String,
    pub content_type: String,
    pub data: Vec<u8>,
}

/// Build an email message from a compose draft.
///
/// # Errors
/// Returns an error if the draft has no recipients, or if the generated
/// message fails header validation.
pub fn build_compose_draft(draft: &ComposeDraft) -> Result<String, VivariumError> {
    build_compose_draft_with_attachments(draft, &[])
}

/// Build an email message from a compose draft with file attachments.
///
/// # Errors
/// Returns an error if the draft has no recipients, or if the generated
/// message fails header validation.
pub fn build_compose_draft_with_attachments(
    draft: &ComposeDraft,
    attachments: &[FileAttachment],
) -> Result<String, VivariumError> {
    if draft.to.is_empty() && draft.cc.is_empty() && draft.bcc.is_empty() {
        return Err(VivariumError::Message(
            "draft needs at least one To, Cc, or Bcc recipient".into(),
        ));
    }
    let mut builder = mail_builder::MessageBuilder::new()
        .from(draft.from.clone())
        .subject(draft.subject.clone())
        .text_body(normalize_body(&draft.body));
    if !draft.to.is_empty() {
        builder = builder.to(draft.to.clone());
    }
    if !draft.cc.is_empty() {
        builder = builder.cc(draft.cc.clone());
    }
    if !draft.bcc.is_empty() {
        builder = builder.bcc(draft.bcc.clone());
    }
    if let Some(html_body) = &draft.html_body {
        builder = builder.html_body(html_body.clone());
    }
    for attachment in attachments {
        builder = builder.attachment(
            attachment.content_type.clone(),
            attachment.filename.clone(),
            attachment.data.as_slice(),
        );
    }
    let eml = builder.write_to_string()?;
    validate_message_headers(eml.as_bytes())?;
    Ok(eml)
}

/// Build a reply email from an original message and reply draft.
///
/// # Errors
/// Returns an error if the original message cannot be parsed, has no From
/// address, or the generated reply fails header validation.
pub fn build_reply(original: &[u8], draft: &ReplyDraft) -> Result<String, VivariumError> {
    let parsed = mail_parser::MessageParser::default()
        .parse(original)
        .ok_or_else(|| VivariumError::Parse("failed to parse original message".into()))?;
    let reply_to = parsed
        .from()
        .and_then(|a| a.first())
        .and_then(|a| a.address())
        .ok_or_else(|| VivariumError::Message("original has no From address".into()))?;
    let subject = reply_subject(parsed.subject().unwrap_or("(no subject)"));
    let text_body = format!("{}{}", normalize_body(&draft.body), quoted_body(&parsed));
    let mut builder = mail_builder::MessageBuilder::new()
        .from(draft.from.clone())
        .to(reply_to.to_string())
        .subject(subject)
        .text_body(text_body);
    if let Some(message_id) = parsed.message_id() {
        builder = builder
            .in_reply_to(message_id.to_string())
            .references(message_id.to_string());
    }
    if let Some(html_body) = &draft.html_body {
        builder = builder.html_body(reply_html_body(html_body, &parsed));
    }
    let eml = builder.write_to_string()?;
    validate_message_headers(eml.as_bytes())?;
    Ok(eml)
}

/// Build a reply template (empty body) from an original message.
///
/// # Errors
/// Returns an error if the original message cannot be parsed or has no From
/// address.
pub fn build_reply_template(original: &[u8], from: &str) -> Result<String, VivariumError> {
    build_reply(
        original,
        &ReplyDraft {
            from: from.into(),
            body: String::new(),
            html_body: None,
        },
    )
}

/// Validate that a raw message has required headers (From and at least one
/// recipient).
///
/// # Errors
/// Returns an error if the message cannot be parsed, has no From header, or
/// has no To, Cc, or Bcc recipient.
pub fn validate_message_headers(data: &[u8]) -> Result<(), VivariumError> {
    let parsed = mail_parser::MessageParser::default()
        .parse(data)
        .ok_or_else(|| VivariumError::Parse("failed to parse message".into()))?;
    if parsed.from().and_then(|a| a.first()).is_none() {
        return Err(VivariumError::Message("message has no From header".into()));
    }
    let has_recipient = parsed.to().is_some_and(|a| a.first().is_some())
        || parsed.cc().is_some_and(|a| a.first().is_some())
        || parsed.bcc().is_some_and(|a| a.first().is_some());
    if !has_recipient {
        return Err(VivariumError::Message(
            "message has no To, Cc, or Bcc recipient".into(),
        ));
    }
    Ok(())
}

/// Replace the From header in a raw message with a new address.
///
/// # Errors
/// Returns an error if `from` is empty or not a valid email address, the
/// message is not UTF-8, has no header/body separator, or has no From header.
pub fn replace_from_header(data: &[u8], from: &str) -> Result<Vec<u8>, VivariumError> {
    let from = from.trim();
    if from.is_empty() {
        return Err(VivariumError::Message("--from cannot be empty".into()));
    }
    let _: lettre::message::Mailbox = from
        .parse()
        .map_err(|e| VivariumError::Message(format!("invalid --from address: {e}")))?;
    let text = std::str::from_utf8(data)
        .map_err(|e| VivariumError::Message(format!("message is not UTF-8: {e}")))?;
    let newline = if text.contains("\r\n") { "\r\n" } else { "\n" };
    let separator = format!("{newline}{newline}");
    let (headers, body) = text
        .split_once(&separator)
        .ok_or_else(|| VivariumError::Message("message has no header/body separator".into()))?;

    let mut found = false;
    let mut skip_continuation = false;
    let mut rewritten = Vec::new();
    for line in headers.split(newline) {
        if skip_continuation && (line.starts_with(' ') || line.starts_with('\t')) {
            continue;
        }
        skip_continuation = false;
        if line
            .split_once(':')
            .is_some_and(|(name, _)| name.eq_ignore_ascii_case("from"))
        {
            if !found {
                rewritten.push(format!("From: {from}"));
                found = true;
                skip_continuation = true;
            }
            continue;
        }
        rewritten.push(line.to_string());
    }
    if !found {
        return Err(VivariumError::Message("message has no From header".into()));
    }
    let rewritten = format!("{}{separator}{body}", rewritten.join(newline));
    validate_message_headers(rewritten.as_bytes())?;
    Ok(rewritten.into_bytes())
}

fn normalize_body(body: &str) -> String {
    let mut body = body.replace('\n', "\r\n");
    if !body.ends_with("\r\n") {
        body.push_str("\r\n");
    }
    body
}

fn quoted_body(parsed: &mail_parser::Message<'_>) -> String {
    parsed
        .body_text(0)
        .map(|body| {
            let quoted = body
                .lines()
                .map(|line| format!("> {line}"))
                .collect::<Vec<_>>()
                .join("\r\n");
            format!("\r\n{quoted}\r\n")
        })
        .unwrap_or_default()
}

fn reply_html_body(html_body: &str, parsed: &mail_parser::Message<'_>) -> String {
    let mut html = html_body.to_string();
    if let Some(body) = parsed.body_text(0) {
        html.push_str("\n<hr style=\"border:0;border-top:1px solid #d7dde5;margin:24px 0;\">\n");
        html.push_str("<blockquote style=\"margin:0;padding-left:16px;border-left:3px solid #c5ced9;color:#4b5563;\">");
        html.push_str(&plain_text_to_html(&body));
        html.push_str("</blockquote>\n");
    }
    html
}

#[must_use] 
pub fn auto_html_body(body: &str) -> String {
    format!("<div dir=\"ltr\">{}</div>\n", plain_text_to_html(body))
}

fn plain_text_to_html(body: &str) -> String {
    escape_html(body).replace('\n', "<br>\n")
}

fn escape_html(text: &str) -> String {
    let mut escaped = String::with_capacity(text.len());
    for ch in text.chars() {
        match ch {
            '&' => escaped.push_str("&amp;"),
            '<' => escaped.push_str("&lt;"),
            '>' => escaped.push_str("&gt;"),
            '"' => escaped.push_str("&quot;"),
            '\'' => escaped.push_str("&#39;"),
            _ => escaped.push(ch),
        }
    }
    escaped
}

fn reply_subject(subject: &str) -> String {
    if subject.starts_with("Re:") {
        subject.to_string()
    } else {
        format!("Re: {subject}")
    }
}

#[cfg(test)]
mod tests {
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
}

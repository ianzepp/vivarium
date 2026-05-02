use crate::error::VivariumError;

#[derive(Debug, Clone)]
pub struct ComposeDraft {
    pub from: String,
    pub to: Vec<String>,
    pub cc: Vec<String>,
    pub bcc: Vec<String>,
    pub subject: String,
    pub body: String,
}

#[derive(Debug, Clone)]
pub struct ReplyDraft {
    pub from: String,
    pub body: String,
}

pub fn build_compose_draft(draft: &ComposeDraft) -> Result<String, VivariumError> {
    if draft.to.is_empty() && draft.cc.is_empty() && draft.bcc.is_empty() {
        return Err(VivariumError::Message(
            "draft needs at least one To, Cc, or Bcc recipient".into(),
        ));
    }
    let mut eml = base_headers(&draft.from, &draft.subject);
    push_address_header(&mut eml, "To", &draft.to);
    push_address_header(&mut eml, "Cc", &draft.cc);
    push_address_header(&mut eml, "Bcc", &draft.bcc);
    eml.push_str("\r\n");
    eml.push_str(&normalize_body(&draft.body));
    validate_message_headers(eml.as_bytes())?;
    Ok(eml)
}

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
    let mut eml = base_headers(&draft.from, &subject);
    push_address_header(&mut eml, "To", &[reply_to.to_string()]);
    if let Some(message_id) = parsed.message_id() {
        let message_id = format!("<{message_id}>");
        eml.push_str(&format!("In-Reply-To: {message_id}\r\n"));
        eml.push_str(&format!("References: {message_id}\r\n"));
    }
    eml.push_str("\r\n");
    eml.push_str(&normalize_body(&draft.body));
    eml.push_str(&quoted_body(&parsed));
    validate_message_headers(eml.as_bytes())?;
    Ok(eml)
}

pub fn build_reply_template(original: &[u8], from: &str) -> Result<String, VivariumError> {
    build_reply(
        original,
        &ReplyDraft {
            from: from.into(),
            body: String::new(),
        },
    )
}

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

fn base_headers(from: &str, subject: &str) -> String {
    let date = chrono::Utc::now().format("%a, %d %b %Y %H:%M:%S %z");
    let message_id = generated_message_id(from);
    format!("From: {from}\r\nSubject: {subject}\r\nDate: {date}\r\nMessage-ID: <{message_id}>\r\n")
}

fn generated_message_id(from: &str) -> String {
    let domain = from
        .rsplit_once('@')
        .map(|(_, domain)| domain)
        .unwrap_or("local");
    format!(
        "{}.{}@{}",
        chrono::Utc::now().timestamp_nanos_opt().unwrap_or_default(),
        std::process::id(),
        domain.trim_matches('>')
    )
}

fn push_address_header(eml: &mut String, name: &str, addresses: &[String]) {
    if !addresses.is_empty() {
        eml.push_str(&format!("{name}: {}\r\n", addresses.join(", ")));
    }
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

    #[test]
    fn compose_draft_includes_required_headers_and_recipients() {
        let eml = build_compose_draft(&ComposeDraft {
            from: "me@example.com".into(),
            to: vec!["a@example.com".into()],
            cc: vec!["b@example.com".into()],
            bcc: vec!["c@example.com".into()],
            subject: "hello".into(),
            body: "body".into(),
        })
        .unwrap();

        assert!(eml.contains("Date: "));
        assert!(eml.contains("Message-ID: <"));
        assert!(eml.contains("To: a@example.com"));
        assert!(eml.contains("Cc: b@example.com"));
        assert!(eml.contains("Bcc: c@example.com"));
        assert!(message_id_from_bytes(eml.as_bytes()).is_some());
    }

    #[test]
    fn reply_draft_threads_to_original() {
        let original = b"Message-ID: <root@example.com>\r\nFrom: A <a@example.com>\r\nTo: me@example.com\r\nSubject: root\r\n\r\nhello";

        let eml = build_reply(
            original,
            &ReplyDraft {
                from: "me@example.com".into(),
                body: "thanks".into(),
            },
        )
        .unwrap();

        assert!(eml.contains("Subject: Re: root"));
        assert!(eml.contains("In-Reply-To: <root@example.com>"));
        assert!(eml.contains("References: <root@example.com>"));
        assert!(eml.contains("> hello"));
    }
}

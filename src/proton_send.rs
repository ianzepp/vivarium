use crate::config::Account;
use crate::error::VivariumError;
use crate::proton_api::{CreateDraftReq, DraftTemplate, ProtonAddress};

pub async fn send_raw(account: &Account, data: &[u8]) -> Result<(), VivariumError> {
    let _draft = draft_request_from_eml(data)?;
    Err(VivariumError::Other(format!(
        "direct Proton API send for account '{}' is not enabled yet: Proton draft package encryption is pending",
        account.name
    )))
}

pub fn draft_request_from_eml(data: &[u8]) -> Result<CreateDraftReq, VivariumError> {
    let parsed = mail_parser::MessageParser::default()
        .parse(data)
        .ok_or_else(|| VivariumError::Parse("failed to parse outbound message".into()))?;
    let sender = first_address("From", parsed.from())?;
    let to = address_list(parsed.to())?;
    let cc = address_list(parsed.cc())?;
    let bcc = address_list(parsed.bcc())?;
    if to.is_empty() && cc.is_empty() && bcc.is_empty() {
        return Err(VivariumError::Message(
            "draft needs at least one To, Cc, or Bcc recipient".into(),
        ));
    }
    let (body, mime_type) = draft_body(&parsed);
    Ok(CreateDraftReq {
        message: DraftTemplate {
            subject: parsed.subject().unwrap_or_default().to_string(),
            sender,
            to,
            cc,
            bcc,
            body,
            mime_type,
            unread: 0,
            external_id: parsed.message_id().map(ToString::to_string),
        },
        attachment_key_packets: Vec::new(),
        parent_id: None,
        action: 0,
    })
}

fn draft_body(parsed: &mail_parser::Message<'_>) -> (String, String) {
    if let Some(part_id) = parsed.html_body.first()
        && let Some(part) = parsed.parts.get(*part_id)
        && let mail_parser::PartType::Html(html) = &part.body
    {
        return (html.to_string(), "text/html".into());
    }
    (
        parsed.body_text(0).unwrap_or_default().to_string(),
        "text/plain".into(),
    )
}

fn first_address(
    label: &str,
    list: Option<&mail_parser::Address<'_>>,
) -> Result<ProtonAddress, VivariumError> {
    address_list(list)?
        .into_iter()
        .next()
        .ok_or_else(|| VivariumError::Message(format!("message has no {label} address")))
}

fn address_list(
    list: Option<&mail_parser::Address<'_>>,
) -> Result<Vec<ProtonAddress>, VivariumError> {
    let Some(list) = list else {
        return Ok(Vec::new());
    };
    Ok(list
        .iter()
        .filter_map(|address| {
            Some(ProtonAddress {
                name: address.name().unwrap_or_default().to_string(),
                address: address.address()?.to_string(),
            })
        })
        .collect())
}

#[cfg(test)]
mod tests {
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
}

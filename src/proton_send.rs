use std::collections::BTreeMap;

use base64::Engine;
use base64::engine::general_purpose::STANDARD;

use crate::config::Account;
use crate::config::Config;
use crate::error::VivariumError;
use crate::proton_api::{
    CreateDraftReq, DraftTemplate, MessagePackage, MessageRecipient, ProtonAddress,
    ProtonApiClient, ProtonSessionStore, SendDraftReq, SessionKey,
};
use crate::proton_encrypt::{ProtonBodyEncryptor, ProtonEncryptedBody};

const CLEAR_SCHEME: u8 = 4;
const NO_SIGNATURE: u8 = 0;

pub async fn send_raw(
    account: &Account,
    config: &Config,
    data: &[u8],
) -> Result<(), VivariumError> {
    let mut draft = draft_request_from_eml(data)?;
    let mail_root = account.mail_path(config);
    let session_store = ProtonSessionStore::new(&mail_root);
    let mut session = session_store.load()?;
    let client = ProtonApiClient::default();

    session = reject_keyed_recipients(&client, &session_store, session, &draft).await?;

    let (refreshed, key_material) = client.key_material(&session).await?;
    session_store.save(&refreshed)?;
    session = refreshed;

    let encryptor = ProtonBodyEncryptor::new(&key_material)?;
    let draft_body = encryptor.encrypt_body(&draft.message.body)?;
    draft.message.body = draft_body.armored_message;

    let (refreshed, created) = client.create_draft(&session, &draft).await?;
    session_store.save(&refreshed)?;
    session = refreshed;

    let package_body = encryptor.encrypt_body(&draft_request_from_eml(data)?.message.body)?;
    let send = clear_send_request(&draft, package_body);
    let (refreshed, _sent) = client.send_draft(&session, &created.id, &send).await?;
    session_store.save(&refreshed)?;
    Ok(())
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

async fn reject_keyed_recipients(
    client: &ProtonApiClient,
    session_store: &ProtonSessionStore,
    mut session: crate::proton_api::ProtonSession,
    draft: &CreateDraftReq,
) -> Result<crate::proton_api::ProtonSession, VivariumError> {
    for recipient in all_recipients(draft) {
        let (refreshed, keys, _recipient_type) = client.public_keys(&session, &recipient).await?;
        if refreshed.access_token != session.access_token {
            session_store.save(&refreshed)?;
        }
        session = refreshed;
        if keys
            .iter()
            .any(|key| key.is_active() && !key.public_key.is_empty())
        {
            return Err(VivariumError::Other(format!(
                "direct Proton API send to keyed recipient '{recipient}' is not enabled yet: encrypted recipient packages are pending"
            )));
        }
    }
    Ok(session)
}

fn clear_send_request(draft: &CreateDraftReq, body: ProtonEncryptedBody) -> SendDraftReq {
    SendDraftReq {
        packages: vec![MessagePackage {
            addresses: serde_json::to_value(clear_recipients(draft)).unwrap_or_default(),
            mime_type: draft.message.mime_type.clone(),
            package_type: CLEAR_SCHEME,
            body: STANDARD.encode(body.data_packet),
            body_key: Some(SessionKey {
                key: STANDARD.encode(body.session_key),
                algorithm: body.algorithm,
            }),
            attachment_keys: Some(serde_json::json!({})),
        }],
    }
}

fn clear_recipients(draft: &CreateDraftReq) -> BTreeMap<String, MessageRecipient> {
    all_recipients(draft)
        .into_iter()
        .map(|address| {
            (
                address,
                MessageRecipient {
                    recipient_type: CLEAR_SCHEME,
                    signature: NO_SIGNATURE,
                    body_key_packet: None,
                    attachment_key_packets: Some(serde_json::json!({})),
                },
            )
        })
        .collect()
}

fn all_recipients(draft: &CreateDraftReq) -> Vec<String> {
    draft
        .message
        .to
        .iter()
        .chain(draft.message.cc.iter())
        .chain(draft.message.bcc.iter())
        .map(|address| address.address.clone())
        .filter(|address| !address.is_empty())
        .collect()
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

    #[test]
    fn clear_send_request_wraps_all_recipients_and_body_key() {
        let data = b"From: sender@example.com\r\nTo: a@example.com\r\nCc: b@example.com\r\nBcc: c@example.com\r\nSubject: Hello\r\n\r\nBody";
        let draft = draft_request_from_eml(data).unwrap();
        let request = clear_send_request(
            &draft,
            ProtonEncryptedBody {
                armored_message: "armored".into(),
                data_packet: b"encrypted".to_vec(),
                session_key: b"session-key".to_vec(),
                algorithm: "aes256".into(),
            },
        );

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
}

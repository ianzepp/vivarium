use std::collections::BTreeMap;

use base64::Engine;
use base64::engine::general_purpose::STANDARD;

use crate::config::Account;
use crate::config::Config;
use crate::error::VivariumError;
use crate::proton_api::{
    CreateDraftReq, DraftTemplate, MessagePackage, MessageRecipient, ProtonAddress,
    ProtonApiClient, ProtonPublicKey, ProtonRecipientType, ProtonSessionStore, SendDraftReq,
    SessionKey,
};
use crate::proton_encrypt::{ProtonBodyEncryptor, ProtonEncryptedBody, encrypt_session_key_packet};

const INTERNAL_SCHEME: u8 = 1;
const CLEAR_SCHEME: u8 = 4;
const PGP_INLINE_SCHEME: u8 = 8;
const NO_SIGNATURE: u8 = 0;
const DETACHED_SIGNATURE: u8 = 1;
const RECIPIENT_TYPE_INTERNAL: ProtonRecipientType = ProtonRecipientType(1);

/// Sends a raw email message through the Proton API.
///
/// # Errors
/// Returns an error if the session cannot be loaded, the message cannot be parsed,
/// encryption fails, or any API call fails.
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

    let (refreshed, recipients) =
        recipient_preferences(&client, &session_store, session, &draft).await?;
    session = refreshed;

    let (refreshed, key_material) = client.key_material(&session).await?;
    session_store.save(&refreshed)?;
    session = refreshed;

    let password = account.resolve_secret().await?;
    let encryptor = ProtonBodyEncryptor::new(&password, &key_material)?;
    let plain_body = draft.message.body.clone();
    let draft_body = encryptor.encrypt_body(&draft.message.body)?;
    draft.message.body = draft_body.armored_message;

    let (refreshed, created) = client.create_draft(&session, &draft).await?;
    session_store.save(&refreshed)?;
    session = refreshed;

    let package_body = encryptor.encrypt_signed_body(&plain_body)?;
    let send = send_request(&draft, package_body, &recipients)?;
    let (refreshed, _sent) = client.send_draft(&session, &created.id, &send).await?;
    session_store.save(&refreshed)?;
    Ok(())
}

/// Builds a `CreateDraftReq` from raw EML bytes.
///
/// # Errors
/// Returns an error if the EML cannot be parsed, required sender/recipient fields are missing,
/// or no To/Cc/Bcc recipients are present.
pub fn draft_request_from_eml(data: &[u8]) -> Result<CreateDraftReq, VivariumError> {
    let parsed = mail_parser::MessageParser::default()
        .parse(data)
        .ok_or_else(|| VivariumError::Parse("failed to parse outbound message".into()))?;
    let sender = first_address("From", parsed.from())?;
    let to = address_list(parsed.to());
    let cc = address_list(parsed.cc());
    let bcc = address_list(parsed.bcc());
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

async fn recipient_preferences(
    client: &ProtonApiClient,
    session_store: &ProtonSessionStore,
    mut session: crate::proton_api::ProtonSession,
    draft: &CreateDraftReq,
) -> Result<
    (
        crate::proton_api::ProtonSession,
        Vec<RecipientSendPreference>,
    ),
    VivariumError,
> {
    let mut preferences = Vec::new();
    for recipient in all_recipients(draft) {
        let (refreshed, keys, recipient_type) = client.public_keys(&session, &recipient).await?;
        if refreshed.access_token != session.access_token {
            session_store.save(&refreshed)?;
        }
        session = refreshed;
        preferences.push(RecipientSendPreference::new(
            recipient,
            active_public_key(&keys),
            recipient_type,
            &draft.message.mime_type,
        )?);
    }
    Ok((session, preferences))
}

fn send_request(
    draft: &CreateDraftReq,
    body: ProtonEncryptedBody,
    recipients: &[RecipientSendPreference],
) -> Result<SendDraftReq, VivariumError> {
    Ok(SendDraftReq {
        packages: vec![MessagePackage {
            addresses: serde_json::to_value(recipients_for_body_key(&body, recipients)?)
                .unwrap_or_default(),
            mime_type: draft.message.mime_type.clone(),
            package_type: package_type(recipients),
            body: STANDARD.encode(body.data_packet),
            body_key: if recipients.iter().any(RecipientSendPreference::is_clear) {
                Some(SessionKey {
                    key: STANDARD.encode(body.session_key),
                    algorithm: body.algorithm,
                })
            } else {
                None
            },
            attachment_keys: Some(serde_json::json!({})),
        }],
    })
}

fn recipients_for_body_key(
    body: &ProtonEncryptedBody,
    recipients: &[RecipientSendPreference],
) -> Result<BTreeMap<String, MessageRecipient>, VivariumError> {
    recipients
        .iter()
        .map(|recipient| {
            Ok((
                recipient.address.clone(),
                MessageRecipient {
                    recipient_type: recipient.scheme,
                    signature: recipient.signature(),
                    body_key_packet: recipient.body_key_packet(&body.session_key)?,
                    attachment_key_packets: Some(serde_json::json!({})),
                },
            ))
        })
        .collect()
}

#[derive(Clone, Debug)]
struct RecipientSendPreference {
    address: String,
    scheme: u8,
    public_key: Option<String>,
}

impl RecipientSendPreference {
    fn new(
        address: String,
        public_key: Option<String>,
        recipient_type: Option<ProtonRecipientType>,
        mime_type: &str,
    ) -> Result<Self, VivariumError> {
        let Some(public_key) = public_key else {
            return Ok(Self::clear(address));
        };
        let scheme = if recipient_type == Some(RECIPIENT_TYPE_INTERNAL) {
            INTERNAL_SCHEME
        } else {
            if mime_type != "text/plain" {
                return Err(VivariumError::Other(format!(
                    "direct Proton API send to external PGP recipient '{address}' requires text/plain until PGP/MIME packages are implemented"
                )));
            }
            PGP_INLINE_SCHEME
        };
        Ok(Self {
            address,
            scheme,
            public_key: Some(public_key),
        })
    }

    fn clear(address: String) -> Self {
        Self {
            address,
            scheme: CLEAR_SCHEME,
            public_key: None,
        }
    }

    fn is_clear(&self) -> bool {
        self.scheme == CLEAR_SCHEME
    }

    fn signature(&self) -> u8 {
        if self.is_clear() {
            NO_SIGNATURE
        } else {
            DETACHED_SIGNATURE
        }
    }

    fn body_key_packet(&self, session_key: &[u8]) -> Result<Option<String>, VivariumError> {
        self.public_key
            .as_deref()
            .map(|key| {
                encrypt_session_key_packet(key, session_key).map(|packet| STANDARD.encode(packet))
            })
            .transpose()
    }
}

fn active_public_key(keys: &[ProtonPublicKey]) -> Option<String> {
    keys.iter()
        .find(|key| key.is_active() && !key.public_key.is_empty())
        .map(|key| key.public_key.clone())
}

fn package_type(recipients: &[RecipientSendPreference]) -> u8 {
    recipients
        .iter()
        .fold(0, |package_type, recipient| package_type | recipient.scheme)
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
    address_list(list)
        .into_iter()
        .next()
        .ok_or_else(|| VivariumError::Message(format!("message has no {label} address")))
}

fn address_list(list: Option<&mail_parser::Address<'_>>) -> Vec<ProtonAddress> {
    let Some(list) = list else {
        return Vec::new();
    };
    list.iter()
        .filter_map(|address| {
            Some(ProtonAddress {
                name: address.name().unwrap_or_default().to_string(),
                address: address.address()?.to_string(),
            })
        })
        .collect()
}

#[cfg(test)]
mod tests;

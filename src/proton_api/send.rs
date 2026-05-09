use serde::{Deserialize, Serialize};

use super::{ProtonAddress, ProtonApiClient, ProtonMessage, ProtonSession, parse_response};
use crate::error::VivariumError;

#[derive(Debug, Serialize)]
pub struct CreateDraftReq {
    #[serde(rename = "Message")]
    pub message: DraftTemplate,
    #[serde(rename = "AttachmentKeyPackets")]
    pub attachment_key_packets: Vec<String>,
    #[serde(rename = "ParentID", skip_serializing_if = "Option::is_none")]
    pub parent_id: Option<String>,
    #[serde(rename = "Action")]
    pub action: u8,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DraftTemplate {
    #[serde(rename = "Subject")]
    pub subject: String,
    #[serde(rename = "Sender")]
    pub sender: ProtonAddress,
    #[serde(rename = "ToList")]
    pub to: Vec<ProtonAddress>,
    #[serde(rename = "CCList")]
    pub cc: Vec<ProtonAddress>,
    #[serde(rename = "BCCList")]
    pub bcc: Vec<ProtonAddress>,
    #[serde(rename = "Body")]
    pub body: String,
    #[serde(rename = "MIMEType")]
    pub mime_type: String,
    #[serde(rename = "Unread")]
    pub unread: u8,
    #[serde(rename = "ExternalID", skip_serializing_if = "Option::is_none")]
    pub external_id: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct SendDraftReq {
    #[serde(rename = "Packages")]
    pub packages: Vec<MessagePackage>,
}

#[derive(Debug, Serialize)]
pub struct MessagePackage {
    #[serde(rename = "Addresses")]
    pub addresses: serde_json::Value,
    #[serde(rename = "MIMEType")]
    pub mime_type: String,
    #[serde(rename = "Type")]
    pub package_type: u8,
    #[serde(rename = "Body")]
    pub body: String,
}

#[derive(Debug, Deserialize)]
pub(super) struct CreateDraftResponse {
    #[serde(rename = "Message")]
    pub message: super::ProtonMessage,
}

#[derive(Debug, Deserialize)]
pub(super) struct SendDraftResponse {
    #[serde(rename = "Sent")]
    pub sent: super::ProtonMessage,
}

impl ProtonApiClient {
    pub async fn create_draft(
        &self,
        session: &ProtonSession,
        request: &CreateDraftReq,
    ) -> Result<(ProtonSession, ProtonMessage), VivariumError> {
        match self.create_draft_with_session(session, request).await? {
            CreateDraftAttempt::Ok(message) => Ok((session.clone(), *message)),
            CreateDraftAttempt::Unauthorized => {
                let refreshed = self.refresh(session).await?;
                let message = match self.create_draft_with_session(&refreshed, request).await? {
                    CreateDraftAttempt::Ok(message) => *message,
                    CreateDraftAttempt::Unauthorized => {
                        return Err(VivariumError::Other(
                            "Proton API create draft was unauthorized after session refresh".into(),
                        ));
                    }
                };
                Ok((refreshed, message))
            }
        }
    }

    pub async fn send_draft(
        &self,
        session: &ProtonSession,
        draft_id: &str,
        request: &SendDraftReq,
    ) -> Result<(ProtonSession, ProtonMessage), VivariumError> {
        match self
            .send_draft_with_session(session, draft_id, request)
            .await?
        {
            SendDraftAttempt::Ok(message) => Ok((session.clone(), *message)),
            SendDraftAttempt::Unauthorized => {
                let refreshed = self.refresh(session).await?;
                let message = match self
                    .send_draft_with_session(&refreshed, draft_id, request)
                    .await?
                {
                    SendDraftAttempt::Ok(message) => *message,
                    SendDraftAttempt::Unauthorized => {
                        return Err(VivariumError::Other(
                            "Proton API send draft was unauthorized after session refresh".into(),
                        ));
                    }
                };
                Ok((refreshed, message))
            }
        }
    }

    async fn create_draft_with_session(
        &self,
        session: &ProtonSession,
        request: &CreateDraftReq,
    ) -> Result<CreateDraftAttempt, VivariumError> {
        let Some(response) = self
            .post_authenticated::<_, CreateDraftResponse>("/mail/v4/messages", session, request)
            .await?
        else {
            return Ok(CreateDraftAttempt::Unauthorized);
        };
        Ok(CreateDraftAttempt::Ok(Box::new(response.message)))
    }

    async fn send_draft_with_session(
        &self,
        session: &ProtonSession,
        draft_id: &str,
        request: &SendDraftReq,
    ) -> Result<SendDraftAttempt, VivariumError> {
        let path = format!("/mail/v4/messages/{draft_id}");
        let Some(response) = self
            .post_authenticated::<_, SendDraftResponse>(&path, session, request)
            .await?
        else {
            return Ok(SendDraftAttempt::Unauthorized);
        };
        Ok(SendDraftAttempt::Ok(Box::new(response.sent)))
    }

    async fn post_authenticated<B: Serialize, T: for<'de> Deserialize<'de>>(
        &self,
        path: &str,
        session: &ProtonSession,
        body: &B,
    ) -> Result<Option<T>, VivariumError> {
        let response = self
            .http
            .post(self.url(path))
            .header("x-pm-appversion", &self.app_version)
            .header("x-pm-uid", &session.uid)
            .bearer_auth(&session.access_token)
            .json(body)
            .send()
            .await
            .map_err(|e| {
                VivariumError::Other(format!("Proton API authenticated request failed: {e}"))
            })?;
        if response.status() == reqwest::StatusCode::UNAUTHORIZED {
            return Ok(None);
        }
        parse_response::<T>(response).await.map(Some).map_err(|e| {
            VivariumError::Other(format!("Proton API authenticated POST {path} failed: {e}"))
        })
    }
}

enum CreateDraftAttempt {
    Ok(Box<ProtonMessage>),
    Unauthorized,
}

enum SendDraftAttempt {
    Ok(Box<ProtonMessage>),
    Unauthorized,
}

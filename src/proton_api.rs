use chrono::Utc;
use reqwest::StatusCode;
use serde::Deserialize;

use crate::error::VivariumError;

mod app_version;
mod auth;
mod identity;
mod keys;
mod messages;
mod send;
mod session;

use app_version::default_app_version;
pub use auth::{AuthInfo, AuthInfoSummary, LoginCheck, TwoFaInfo};
use auth::{AuthInfoRequest, AuthRefreshRequest, AuthRequest, AuthResponse};
pub use identity::ProtonIdentity;
use identity::{AddressListResponse, UserResponse};
use keys::{AddressKeyListResponse, KeySaltResponse, UserKeyResponse};
pub use keys::{ProtonAddressKeyMaterial, ProtonKeyMaterial, ProtonKeySalt, ProtonUserKeyMaterial};
use messages::{FullMessageResponse, MessageListResponse};
pub use messages::{ProtonAddress, ProtonFullMessage, ProtonMessage};
pub use send::{CreateDraftReq, DraftTemplate, MessagePackage, SendDraftReq};
pub use session::{ProtonSession, ProtonSessionStore};

const DEFAULT_BASE_URL: &str = "https://mail.proton.me/api";

#[derive(Clone)]
pub struct ProtonApiClient {
    base_url: String,
    app_version: String,
    http: reqwest::Client,
}

impl Default for ProtonApiClient {
    fn default() -> Self {
        Self::new(DEFAULT_BASE_URL)
    }
}

impl ProtonApiClient {
    pub fn new(base_url: impl Into<String>) -> Self {
        Self {
            base_url: base_url.into().trim_end_matches('/').to_string(),
            app_version: std::env::var("VIVI_PROTON_APP_VERSION")
                .unwrap_or_else(|_| default_app_version()),
            http: reqwest::Client::new(),
        }
    }

    pub async fn auth_info(&self, username: &str) -> Result<AuthInfo, VivariumError> {
        let response = self
            .http
            .post(self.url("/auth/v4/info"))
            .header("x-pm-appversion", &self.app_version)
            .json(&AuthInfoRequest { username })
            .send()
            .await
            .map_err(|e| {
                VivariumError::Other(format!("Proton API auth-info request failed: {e}"))
            })?;
        parse_response::<AuthInfo>(response).await
    }

    pub async fn login_check(
        &self,
        username: &str,
        password: &str,
        totp_code: Option<&str>,
    ) -> Result<LoginCheck, VivariumError> {
        self.login(username, password, totp_code)
            .await
            .map(|session| session.check())
    }

    pub async fn login(
        &self,
        username: &str,
        password: &str,
        totp_code: Option<&str>,
    ) -> Result<ProtonSession, VivariumError> {
        let auth_info = self.auth_info(username).await?;
        let proof = auth_info.proof(username, password)?;
        let response = self
            .http
            .post(self.url("/auth/v4"))
            .header("x-pm-appversion", &self.app_version)
            .json(&AuthRequest {
                username,
                client_ephemeral: &proof.client_ephemeral,
                client_proof: &proof.client_proof,
                srp_session: &auth_info.srp_session,
                two_factor_code: totp_code,
            })
            .send()
            .await
            .map_err(|e| VivariumError::Other(format!("Proton API login request failed: {e}")))?;
        let auth = parse_response::<AuthResponse>(response).await?;
        if !proof.compare_server_proof(&auth.server_proof) {
            return Err(VivariumError::Other(
                "Proton API login returned an invalid server proof".into(),
            ));
        }
        auth.into_session(self.app_version.clone())
    }

    pub async fn refresh(&self, session: &ProtonSession) -> Result<ProtonSession, VivariumError> {
        let response = self
            .http
            .post(self.url("/auth/v4/refresh"))
            .header("x-pm-appversion", &self.app_version)
            .header("x-pm-uid", &session.uid)
            .bearer_auth(&session.access_token)
            .json(&AuthRefreshRequest {
                uid: &session.uid,
                refresh_token: &session.refresh_token,
                response_type: "token",
                grant_type: "refresh_token",
                redirect_uri: "https://protonmail.ch",
                state: &refresh_state(),
                access_token: &session.access_token,
            })
            .send()
            .await
            .map_err(|e| VivariumError::Other(format!("Proton API refresh request failed: {e}")))?;
        let mut refreshed = parse_response::<AuthResponse>(response)
            .await?
            .into_session(self.app_version.clone())?;
        refreshed.preserve_metadata_from(session);
        Ok(refreshed)
    }

    pub async fn identity(
        &self,
        session: &ProtonSession,
    ) -> Result<(ProtonSession, ProtonIdentity), VivariumError> {
        match self.identity_with_session(session).await? {
            IdentityAttempt::Ok(identity) => Ok((session.clone(), identity)),
            IdentityAttempt::Unauthorized => {
                let refreshed = self.refresh(session).await?;
                let identity = match self.identity_with_session(&refreshed).await? {
                    IdentityAttempt::Ok(identity) => identity,
                    IdentityAttempt::Unauthorized => {
                        return Err(VivariumError::Other(
                            "Proton API identity request was unauthorized after session refresh"
                                .into(),
                        ));
                    }
                };
                Ok((refreshed, identity))
            }
        }
    }

    pub async fn list_messages(
        &self,
        session: &ProtonSession,
        page: usize,
        page_size: usize,
    ) -> Result<(ProtonSession, Vec<ProtonMessage>, usize), VivariumError> {
        match self
            .message_page_with_session(session, page, page_size)
            .await?
        {
            MessagePageAttempt::Ok(page) => Ok((session.clone(), page.messages, page.total)),
            MessagePageAttempt::Unauthorized => {
                let refreshed = self.refresh(session).await?;
                let page = match self
                    .message_page_with_session(&refreshed, page, page_size)
                    .await?
                {
                    MessagePageAttempt::Ok(page) => page,
                    MessagePageAttempt::Unauthorized => {
                        return Err(VivariumError::Other(
                            "Proton API message list was unauthorized after session refresh".into(),
                        ));
                    }
                };
                Ok((refreshed, page.messages, page.total))
            }
        }
    }

    pub async fn fetch_message(
        &self,
        session: &ProtonSession,
        id: &str,
    ) -> Result<(ProtonSession, ProtonFullMessage), VivariumError> {
        match self.full_message_with_session(session, id).await? {
            FullMessageAttempt::Ok(message) => Ok((session.clone(), *message)),
            FullMessageAttempt::Unauthorized => {
                let refreshed = self.refresh(session).await?;
                let message = match self.full_message_with_session(&refreshed, id).await? {
                    FullMessageAttempt::Ok(message) => *message,
                    FullMessageAttempt::Unauthorized => {
                        return Err(VivariumError::Other(
                            "Proton API message fetch was unauthorized after session refresh"
                                .into(),
                        ));
                    }
                };
                Ok((refreshed, message))
            }
        }
    }

    pub async fn key_material(
        &self,
        session: &ProtonSession,
    ) -> Result<(ProtonSession, ProtonKeyMaterial), VivariumError> {
        match self.key_material_with_session(session).await? {
            KeyMaterialAttempt::Ok(material) => Ok((session.clone(), material)),
            KeyMaterialAttempt::Unauthorized => {
                let refreshed = self.refresh(session).await?;
                let material = match self.key_material_with_session(&refreshed).await? {
                    KeyMaterialAttempt::Ok(material) => material,
                    KeyMaterialAttempt::Unauthorized => {
                        return Err(VivariumError::Other(
                            "Proton API key material request was unauthorized after session refresh"
                                .into(),
                        ));
                    }
                };
                Ok((refreshed, material))
            }
        }
    }

    async fn message_page_with_session(
        &self,
        session: &ProtonSession,
        page: usize,
        page_size: usize,
    ) -> Result<MessagePageAttempt, VivariumError> {
        let path = format!("/mail/v4/messages?Page={page}&PageSize={page_size}");
        let Some(page) = self
            .get_authenticated::<MessageListResponse>(&path, session)
            .await?
        else {
            return Ok(MessagePageAttempt::Unauthorized);
        };
        Ok(MessagePageAttempt::Ok(page))
    }

    async fn full_message_with_session(
        &self,
        session: &ProtonSession,
        id: &str,
    ) -> Result<FullMessageAttempt, VivariumError> {
        let path = format!("/mail/v4/messages/{id}");
        let Some(response) = self
            .get_authenticated::<FullMessageResponse>(&path, session)
            .await?
        else {
            return Ok(FullMessageAttempt::Unauthorized);
        };
        Ok(FullMessageAttempt::Ok(Box::new(response.message)))
    }

    async fn identity_with_session(
        &self,
        session: &ProtonSession,
    ) -> Result<IdentityAttempt, VivariumError> {
        let Some(user) = self
            .get_authenticated::<UserResponse>("/core/v4/users", session)
            .await?
        else {
            return Ok(IdentityAttempt::Unauthorized);
        };
        let Some(addresses) = self
            .get_authenticated::<AddressListResponse>("/core/v4/addresses", session)
            .await?
        else {
            return Ok(IdentityAttempt::Unauthorized);
        };
        Ok(IdentityAttempt::Ok(ProtonIdentity::from_responses(
            user, addresses,
        )))
    }

    async fn key_material_with_session(
        &self,
        session: &ProtonSession,
    ) -> Result<KeyMaterialAttempt, VivariumError> {
        let Some(user) = self
            .get_authenticated::<UserKeyResponse>("/core/v4/users", session)
            .await?
        else {
            return Ok(KeyMaterialAttempt::Unauthorized);
        };
        let Some(addresses) = self
            .get_authenticated::<AddressKeyListResponse>("/core/v4/addresses", session)
            .await?
        else {
            return Ok(KeyMaterialAttempt::Unauthorized);
        };
        let Some(salts) = self
            .get_authenticated::<KeySaltResponse>("/core/v4/keys/salts", session)
            .await?
        else {
            return Ok(KeyMaterialAttempt::Unauthorized);
        };
        Ok(KeyMaterialAttempt::Ok(ProtonKeyMaterial::from_responses(
            user, addresses, salts,
        )))
    }

    async fn get_authenticated<T: for<'de> Deserialize<'de>>(
        &self,
        path: &str,
        session: &ProtonSession,
    ) -> Result<Option<T>, VivariumError> {
        let response = self
            .http
            .get(self.url(path))
            .header("x-pm-appversion", &self.app_version)
            .header("x-pm-uid", &session.uid)
            .bearer_auth(&session.access_token)
            .send()
            .await
            .map_err(|e| {
                VivariumError::Other(format!("Proton API authenticated request failed: {e}"))
            })?;
        if response.status() == StatusCode::UNAUTHORIZED {
            return Ok(None);
        }
        parse_response::<T>(response).await.map(Some).map_err(|e| {
            VivariumError::Other(format!("Proton API authenticated GET {path} failed: {e}"))
        })
    }

    fn url(&self, path: &str) -> String {
        format!("{}{}", self.base_url, path)
    }
}

enum IdentityAttempt {
    Ok(ProtonIdentity),
    Unauthorized,
}

enum MessagePageAttempt {
    Ok(MessageListResponse),
    Unauthorized,
}

enum FullMessageAttempt {
    Ok(Box<ProtonFullMessage>),
    Unauthorized,
}

enum KeyMaterialAttempt {
    Ok(ProtonKeyMaterial),
    Unauthorized,
}

#[derive(Deserialize)]
struct ApiError {
    #[serde(rename = "Error")]
    error: Option<String>,
    #[serde(rename = "Details")]
    details: Option<String>,
}

async fn parse_response<T: for<'de> Deserialize<'de>>(
    response: reqwest::Response,
) -> Result<T, VivariumError> {
    let status = response.status();
    if !status.is_success() {
        let body = response.text().await.unwrap_or_default();
        let reason = serde_json::from_str::<ApiError>(&body)
            .ok()
            .and_then(|api| api.error.or(api.details))
            .filter(|value| !value.is_empty())
            .unwrap_or(body);
        return Err(VivariumError::Other(format!(
            "Proton API returned {status}: {reason}"
        )));
    }
    let body = response
        .text()
        .await
        .map_err(|e| VivariumError::Other(format!("Proton API response body read failed: {e}")))?;
    serde_json::from_str::<T>(&body)
        .map_err(|e| VivariumError::Other(format!("Proton API response JSON failed: {e}")))
}

fn refresh_state() -> String {
    let now = Utc::now()
        .timestamp_nanos_opt()
        .unwrap_or_else(|| Utc::now().timestamp_micros());
    format!("vivi-{now}")
}

#[cfg(test)]
mod send_tests;
#[cfg(test)]
mod tests;

use chrono::Utc;
use proton_srp::{SRPAuth, SRPProofB64, SrpHashVersion};
use reqwest::StatusCode;
use serde::{Deserialize, Serialize};

use crate::error::VivariumError;

mod identity;
mod session;

pub use identity::ProtonIdentity;
use identity::{AddressListResponse, UserResponse};
pub use session::{ProtonSession, ProtonSessionStore};

const DEFAULT_BASE_URL: &str = "https://mail.proton.me/api";
const DEFAULT_APP_VERSION: &str = "web-mail@5.0.113.4";

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
                .unwrap_or_else(|_| DEFAULT_APP_VERSION.into()),
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

    async fn identity_with_session(
        &self,
        session: &ProtonSession,
    ) -> Result<IdentityAttempt, VivariumError> {
        let Some(user) = self
            .get_authenticated::<UserResponse>("/users", session)
            .await?
        else {
            return Ok(IdentityAttempt::Unauthorized);
        };
        let Some(addresses) = self
            .get_authenticated::<AddressListResponse>("/addresses", session)
            .await?
        else {
            return Ok(IdentityAttempt::Unauthorized);
        };
        Ok(IdentityAttempt::Ok(ProtonIdentity::from_responses(
            user, addresses,
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
        parse_response::<T>(response).await.map(Some)
    }

    fn url(&self, path: &str) -> String {
        format!("{}{}", self.base_url, path)
    }
}

enum IdentityAttempt {
    Ok(ProtonIdentity),
    Unauthorized,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthInfo {
    #[serde(rename = "Version")]
    pub version: u8,
    #[serde(rename = "Modulus")]
    pub modulus: String,
    #[serde(rename = "ServerEphemeral")]
    pub server_ephemeral: String,
    #[serde(rename = "Salt")]
    pub salt: String,
    #[serde(rename = "SRPSession")]
    pub srp_session: String,
    #[serde(rename = "2FA", default)]
    pub two_fa: TwoFaInfo,
}

impl AuthInfo {
    pub fn proof(&self, username: &str, password: &str) -> Result<SRPProofB64, VivariumError> {
        let version = SrpHashVersion::try_from(self.version).map_err(|e| {
            VivariumError::Other(format!(
                "Proton API returned unsupported SRP version {}: {e}",
                self.version
            ))
        })?;
        let auth = SRPAuth::with_pgp(
            Some(username),
            password,
            version,
            &self.salt,
            &self.modulus,
            &self.server_ephemeral,
        )
        .map_err(|e| VivariumError::Other(format!("Proton SRP proof setup failed: {e}")))?;
        auth.generate_proofs()
            .map(SRPProofB64::from)
            .map_err(|e| VivariumError::Other(format!("Proton SRP proof generation failed: {e}")))
    }

    pub fn summary(&self) -> AuthInfoSummary {
        AuthInfoSummary {
            version: self.version,
            srp_session_present: !self.srp_session.is_empty(),
            modulus_present: !self.modulus.is_empty(),
            server_ephemeral_present: !self.server_ephemeral.is_empty(),
            salt_present: !self.salt.is_empty(),
            two_fa: self.two_fa.clone(),
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TwoFaInfo {
    #[serde(rename = "Enabled")]
    pub enabled: u8,
}

#[derive(Debug, Serialize)]
pub struct AuthInfoSummary {
    pub version: u8,
    pub srp_session_present: bool,
    pub modulus_present: bool,
    pub server_ephemeral_present: bool,
    pub salt_present: bool,
    pub two_fa: TwoFaInfo,
}

#[derive(Debug, Serialize)]
pub struct LoginCheck {
    pub uid_present: bool,
    pub user_id_present: bool,
    pub scope: String,
    pub password_mode: u8,
    pub two_fa: TwoFaInfo,
    pub app_version: String,
    pub updated_at: String,
}

#[derive(Serialize)]
#[serde(rename_all = "PascalCase")]
struct AuthInfoRequest<'a> {
    username: &'a str,
}

#[derive(Serialize)]
#[serde(rename_all = "PascalCase")]
struct AuthRequest<'a> {
    username: &'a str,
    client_ephemeral: &'a str,
    client_proof: &'a str,
    #[serde(rename = "SRPSession")]
    srp_session: &'a str,
    #[serde(rename = "TwoFactorCode", skip_serializing_if = "Option::is_none")]
    two_factor_code: Option<&'a str>,
}

#[derive(Serialize)]
#[serde(rename_all = "PascalCase")]
struct AuthRefreshRequest<'a> {
    #[serde(rename = "UID")]
    uid: &'a str,
    refresh_token: &'a str,
    response_type: &'a str,
    grant_type: &'a str,
    redirect_uri: &'a str,
    state: &'a str,
    access_token: &'a str,
}

#[derive(Deserialize)]
struct AuthResponse {
    #[serde(rename = "UID", default)]
    uid: String,
    #[serde(rename = "UserID", default)]
    user_id: String,
    #[serde(rename = "AccessToken", default)]
    access_token: String,
    #[serde(rename = "RefreshToken", default)]
    refresh_token: String,
    #[serde(rename = "Scope", default)]
    scope: String,
    #[serde(rename = "ServerProof", default)]
    server_proof: String,
    #[serde(rename = "2FA", default)]
    two_fa: TwoFaInfo,
    #[serde(rename = "PasswordMode", default)]
    password_mode: u8,
}

impl AuthResponse {
    fn into_session(self, app_version: String) -> Result<ProtonSession, VivariumError> {
        if self.uid.is_empty() || self.access_token.is_empty() || self.refresh_token.is_empty() {
            return Err(VivariumError::Other(
                "Proton API auth response did not include complete session tokens".into(),
            ));
        }
        Ok(ProtonSession {
            uid: self.uid,
            access_token: self.access_token,
            refresh_token: self.refresh_token,
            app_version,
            user_id: self.user_id,
            scope: self.scope,
            password_mode: self.password_mode,
            two_fa: self.two_fa,
            updated_at: Utc::now().to_rfc3339(),
        })
    }
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
    response
        .json::<T>()
        .await
        .map_err(|e| VivariumError::Other(format!("Proton API response JSON failed: {e}")))
}

fn refresh_state() -> String {
    let now = Utc::now()
        .timestamp_nanos_opt()
        .unwrap_or_else(|| Utc::now().timestamp_micros());
    format!("vivi-{now}")
}

#[cfg(test)]
mod tests;

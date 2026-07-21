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
    /// Computes the SRP proof for authentication.
    ///
    /// # Errors
    /// Returns an error if the SRP version is unsupported, proof setup fails, or proof generation fails.
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

    #[must_use] 
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
#[allow(clippy::struct_excessive_bools)]
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
pub(super) struct AuthInfoRequest<'a> {
    pub(super) username: &'a str,
}

#[derive(Serialize)]
#[serde(rename_all = "PascalCase")]
pub(super) struct AuthRequest<'a> {
    pub(super) username: &'a str,
    pub(super) client_ephemeral: &'a str,
    pub(super) client_proof: &'a str,
    #[serde(rename = "SRPSession")]
    pub(super) srp_session: &'a str,
    #[serde(rename = "TwoFactorCode", skip_serializing_if = "Option::is_none")]
    pub(super) two_factor_code: Option<&'a str>,
}

#[derive(Serialize)]
#[serde(rename_all = "PascalCase")]
pub(super) struct AuthRefreshRequest<'a> {
    #[serde(rename = "UID")]
    pub(super) uid: &'a str,
    pub(super) refresh_token: &'a str,
    pub(super) response_type: &'a str,
    pub(super) grant_type: &'a str,
    pub(super) redirect_uri: &'a str,
    pub(super) state: &'a str,
    pub(super) access_token: &'a str,
}

#[derive(Deserialize)]
pub(super) struct AuthResponse {
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
    pub(super) server_proof: String,
    #[serde(rename = "2FA", default)]
    two_fa: TwoFaInfo,
    #[serde(rename = "PasswordMode", default)]
    password_mode: u8,
}

impl AuthResponse {
    pub(super) fn into_session(self, app_version: String) -> Result<ProtonSession, VivariumError> {
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
use chrono::Utc;
use proton_srp::{SRPAuth, SRPProofB64, SrpHashVersion};
use serde::{Deserialize, Serialize};

use super::ProtonSession;
use crate::error::VivariumError;

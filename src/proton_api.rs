use proton_srp::{SRPAuth, SRPProofB64, SrpHashVersion};
use serde::{Deserialize, Serialize};

use crate::error::VivariumError;

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
        Ok(LoginCheck {
            user_id_present: !auth.user_id.is_empty(),
            scope: auth.scope,
            password_mode: auth.password_mode,
            two_fa: auth.two_fa,
        })
    }

    fn url(&self, path: &str) -> String {
        format!("{}{}", self.base_url, path)
    }
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
    pub user_id_present: bool,
    pub scope: String,
    pub password_mode: u8,
    pub two_fa: TwoFaInfo,
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

#[derive(Deserialize)]
struct AuthResponse {
    #[serde(rename = "UserID", default)]
    user_id: String,
    #[serde(rename = "Scope", default)]
    scope: String,
    #[serde(rename = "ServerProof")]
    server_proof: String,
    #[serde(rename = "2FA", default)]
    two_fa: TwoFaInfo,
    #[serde(rename = "PasswordMode", default)]
    password_mode: u8,
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

#[cfg(test)]
mod tests {
    use serde_json::Value;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;
    use tokio::sync::oneshot;

    use super::ProtonApiClient;

    #[tokio::test]
    async fn auth_info_posts_username_without_secret_material() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let endpoint = format!("http://{}", listener.local_addr().unwrap());
        let (tx, rx) = oneshot::channel();

        tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let request = read_http_request(&mut stream).await;
            let _ = tx.send(request);
            let body = r#"{"Version":4,"Modulus":"m","ServerEphemeral":"s","Salt":"salt","SRPSession":"session"}"#;
            let response = format!(
                "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\n\r\n{body}",
                body.len()
            );
            stream.write_all(response.as_bytes()).await.unwrap();
        });

        let auth_info = ProtonApiClient::new(endpoint)
            .auth_info("agent@proton.me")
            .await
            .unwrap();

        assert_eq!(auth_info.version, 4);
        let request = rx.await.unwrap();
        assert!(request.starts_with("POST /auth/v4/info HTTP/1.1"));
        assert!(request.contains("x-pm-appversion: web-mail@"));
        let body = request.split("\r\n\r\n").nth(1).unwrap();
        let body: Value = serde_json::from_str(body).unwrap();
        assert_eq!(body["Username"], "agent@proton.me");
        assert!(body.get("Password").is_none());
        assert!(body.get("ClientProof").is_none());
    }

    async fn read_http_request(stream: &mut tokio::net::TcpStream) -> String {
        let mut buffer = vec![0; 8192];
        let n = stream.read(&mut buffer).await.unwrap();
        String::from_utf8_lossy(&buffer[..n]).to_string()
    }
}

use super::{
    ProtonApiClient, ProtonPublicKey, ProtonRecipientType, ProtonSession, keys::PublicKeyResponse,
    parse_response,
};
use crate::error::VivariumError;

impl ProtonApiClient {
    pub async fn public_keys(
        &self,
        session: &ProtonSession,
        address: &str,
    ) -> Result<
        (
            ProtonSession,
            Vec<ProtonPublicKey>,
            Option<ProtonRecipientType>,
        ),
        VivariumError,
    > {
        match self.public_keys_with_session(session, address).await? {
            PublicKeysAttempt::Ok(keys, recipient_type) => {
                Ok((session.clone(), keys, recipient_type))
            }
            PublicKeysAttempt::Unauthorized => {
                let refreshed = self.refresh(session).await?;
                let (keys, recipient_type) =
                    match self.public_keys_with_session(&refreshed, address).await? {
                        PublicKeysAttempt::Ok(keys, recipient_type) => (keys, recipient_type),
                        PublicKeysAttempt::Unauthorized => {
                            return Err(VivariumError::Other(
                            "Proton API public key request was unauthorized after session refresh"
                                .into(),
                        ));
                        }
                    };
                Ok((refreshed, keys, recipient_type))
            }
        }
    }

    async fn public_keys_with_session(
        &self,
        session: &ProtonSession,
        address: &str,
    ) -> Result<PublicKeysAttempt, VivariumError> {
        let response = self
            .http
            .get(self.url("/core/v4/keys"))
            .header("x-pm-appversion", &self.app_version)
            .header("x-pm-uid", &session.uid)
            .bearer_auth(&session.access_token)
            .query(&[("Email", address)])
            .send()
            .await
            .map_err(|e| {
                VivariumError::Other(format!("Proton API public key request failed: {e}"))
            })?;
        if response.status() == reqwest::StatusCode::UNAUTHORIZED {
            return Ok(PublicKeysAttempt::Unauthorized);
        }
        let response = parse_response::<PublicKeyResponse>(response)
            .await
            .map_err(|e| {
                VivariumError::Other(format!("Proton API public key request failed: {e}"))
            })?;
        Ok(PublicKeysAttempt::Ok(
            response.keys,
            response.recipient_type,
        ))
    }
}

enum PublicKeysAttempt {
    Ok(Vec<ProtonPublicKey>, Option<ProtonRecipientType>),
    Unauthorized,
}

use base64::Engine;
use base64::engine::general_purpose::{STANDARD, STANDARD_NO_PAD, URL_SAFE, URL_SAFE_NO_PAD};
use pgp::composed::{Deserializable, Message, SignedSecretKey};
use pgp::types::Password;
use proton_srp::mailbox_password_hash;
use std::io::Cursor;

use crate::error::VivariumError;
use crate::proton_api::ProtonKeyMaterial;

const PROTON_SALT_SUFFIX: &[u8] = b"proton";

pub struct ProtonBodyDecryptor {
    address_keys: Vec<UnlockedAddressKey>,
}

pub(crate) struct UnlockedAddressKey {
    pub(crate) key: SignedSecretKey,
    pub(crate) password: Vec<u8>,
}

impl ProtonBodyDecryptor {
    /// Creates a new body decryptor from the login password and key material.
    ///
    /// # Errors
    /// Returns an error if the key material cannot unlock any address keys.
    pub fn new(
        login_password: &str,
        key_material: &ProtonKeyMaterial,
    ) -> Result<Self, VivariumError> {
        let address_keys = unlock_address_keys(login_password, key_material)?;
        Ok(Self { address_keys })
    }

    /// Decrypts an armored PGP message body.
    ///
    /// # Errors
    /// Returns an error if none of the address keys can decrypt the body.
    pub fn decrypt_body(&self, armored_body: &str) -> Result<Vec<u8>, VivariumError> {
        for address_key in &self.address_keys {
            let password = Password::from(address_key.password.as_slice());
            if let Ok(bytes) = decrypt_armored_message(armored_body, &password, &address_key.key) {
                return Ok(bytes);
            }
        }
        Err(VivariumError::Other(
            "Proton message body could not be decrypted with available address keys".into(),
        ))
    }
}

pub(crate) fn unlock_address_keys(
    login_password: &str,
    key_material: &ProtonKeyMaterial,
) -> Result<Vec<UnlockedAddressKey>, VivariumError> {
    let mut address_keys = Vec::new();
    let mut diagnostics = UnlockDiagnostics::new(key_material);

    for user_key in &key_material.user_keys {
        let Some(key_salt) = key_material
            .key_salts
            .iter()
            .find(|salt| salt.key_id == user_key.id)
        else {
            diagnostics.user_keys_without_salt += 1;
            continue;
        };
        let Ok(mailbox_password) = derive_mailbox_password(&key_salt.key_salt, login_password)
        else {
            diagnostics.mailbox_hash_failures += 1;
            continue;
        };
        for address_key in sorted_address_keys(key_material) {
            diagnostics.unlock_attempts += 1;
            let password = if let (Some(token), Some(_signature)) =
                (&address_key.token, &address_key.signature)
            {
                unlock_address_key_password(&user_key.private_key, &mailbox_password, token)
                    .unwrap_or_else(|err| {
                        diagnostics.token_decrypt_failures += 1;
                        diagnostics.last_token_decrypt_error = Some(err.to_string());
                        mailbox_password.clone()
                    })
            } else {
                mailbox_password.clone()
            };
            let Ok(key) = read_secret_key(&address_key.private_key) else {
                diagnostics.address_key_parse_failures += 1;
                continue;
            };
            address_keys.push(UnlockedAddressKey { key, password });
        }
    }

    if address_keys.is_empty() {
        return Err(VivariumError::Other(diagnostics.error_message()));
    }

    Ok(address_keys)
}

struct UnlockDiagnostics {
    user_keys: usize,
    address_keys: usize,
    key_salts: usize,
    user_keys_without_salt: usize,
    mailbox_hash_failures: usize,
    unlock_attempts: usize,
    token_decrypt_failures: usize,
    address_key_parse_failures: usize,
    last_token_decrypt_error: Option<String>,
}

impl UnlockDiagnostics {
    fn new(key_material: &ProtonKeyMaterial) -> Self {
        Self {
            user_keys: key_material.user_keys.len(),
            address_keys: key_material.address_keys.len(),
            key_salts: key_material.key_salts.len(),
            user_keys_without_salt: 0,
            mailbox_hash_failures: 0,
            unlock_attempts: 0,
            token_decrypt_failures: 0,
            address_key_parse_failures: 0,
            last_token_decrypt_error: None,
        }
    }

    fn error_message(&self) -> String {
        format!(
            "Proton key material could not unlock any address keys: user_keys={}, address_keys={}, key_salts={}, user_keys_without_salt={}, mailbox_hash_failures={}, unlock_attempts={}, token_decrypt_failures={}, address_key_parse_failures={}, last_token_decrypt_error={}",
            self.user_keys,
            self.address_keys,
            self.key_salts,
            self.user_keys_without_salt,
            self.mailbox_hash_failures,
            self.unlock_attempts,
            self.token_decrypt_failures,
            self.address_key_parse_failures,
            self.last_token_decrypt_error.as_deref().unwrap_or("none")
        )
    }
}

/// Derives the mailbox password from the encoded salt and login password.
///
/// # Errors
/// Returns an error if the salt cannot be decoded, is an unsupported length,
/// or the password hash fails.
pub fn derive_mailbox_password(
    encoded_salt: &str,
    login_password: &str,
) -> Result<Vec<u8>, VivariumError> {
    let salt = normalized_mailbox_salt(encoded_salt)?;
    let hash = mailbox_password_hash(login_password, &salt)
        .map_err(|e| VivariumError::Other(format!("Proton mailbox password hash failed: {e}")))?;
    let bytes = hash.as_bytes();
    if bytes.len() < 31 {
        return Err(VivariumError::Other(
            "Proton mailbox password hash was shorter than expected".into(),
        ));
    }
    Ok(bytes[bytes.len() - 31..].to_vec())
}

fn unlock_address_key_password(
    user_private_key: &str,
    mailbox_password: &[u8],
    token: &str,
) -> Result<Vec<u8>, VivariumError> {
    let user_key = read_secret_key(user_private_key)?;
    let password = Password::from(mailbox_password);
    decrypt_armored_message(token, &password, &user_key)
}

fn decrypt_armored_message(
    armored_message: &str,
    password: &Password,
    key: &SignedSecretKey,
) -> Result<Vec<u8>, VivariumError> {
    let parse_message = || parse_pgp_message(armored_message);
    let mut message = match parse_message()?.decrypt(password, key) {
        Ok(message) => message,
        Err(_) => parse_message()?
            .decrypt_legacy(password, key)
            .map_err(|e| VivariumError::Other(format!("Proton PGP message decrypt failed: {e}")))?,
    };
    if message.is_compressed() {
        message = message.decompress().map_err(|e| {
            VivariumError::Other(format!("Proton PGP message decompress failed: {e}"))
        })?;
    }
    message
        .as_data_vec()
        .map_err(|e| VivariumError::Other(format!("Proton PGP message payload read failed: {e}")))
}

fn parse_pgp_message(message: &str) -> Result<Message<'_>, VivariumError> {
    if let Ok((message, _)) = Message::from_armor(message.as_bytes()) {
        return Ok(message);
    }
    if let Ok(message) = Message::from_bytes(message.as_bytes()) {
        return Ok(message);
    }
    let decoded = decode_base64_message(message.trim()).map_err(|e| {
        VivariumError::Other(format!("Proton PGP message base64 decode failed: {e}"))
    })?;
    Message::from_bytes(Cursor::new(decoded))
        .map_err(|e| VivariumError::Other(format!("Proton PGP message parse failed: {e}")))
}

fn decode_base64_message(message: &str) -> Result<Vec<u8>, base64::DecodeError> {
    STANDARD
        .decode(message)
        .or_else(|_| STANDARD_NO_PAD.decode(message))
        .or_else(|_| URL_SAFE.decode(message))
        .or_else(|_| URL_SAFE_NO_PAD.decode(message))
}

pub(crate) fn read_secret_key(armored_key: &str) -> Result<SignedSecretKey, VivariumError> {
    SignedSecretKey::from_reader_single(armored_key.as_bytes())
        .map(|(key, _)| key)
        .map_err(|e| VivariumError::Other(format!("Proton PGP private key parse failed: {e}")))
}

pub(crate) fn sorted_address_keys(
    key_material: &ProtonKeyMaterial,
) -> Vec<&crate::proton_api::ProtonAddressKeyMaterial> {
    let mut keys: Vec<_> = key_material.address_keys.iter().collect();
    keys.sort_by_key(|key| (!key.active, !key.primary, key.address.clone()));
    keys
}

fn normalized_mailbox_salt(encoded: &str) -> Result<Vec<u8>, VivariumError> {
    let mut salt = STANDARD
        .decode(encoded)
        .map_err(|e| VivariumError::Other(format!("Proton auth salt decode failed: {e}")))?;
    if salt.len() == 10 {
        salt.extend_from_slice(PROTON_SALT_SUFFIX);
    }
    if salt.len() != 16 {
        return Err(VivariumError::Other(format!(
            "Proton auth salt has unsupported length {}",
            salt.len()
        )));
    }
    Ok(salt)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::proton_api::{
        ProtonAddressKeyMaterial, ProtonKeyMaterial, ProtonKeySalt, ProtonUserKeyMaterial,
    };
    use base64::engine::general_purpose::STANDARD;

    #[test]
    fn mailbox_salt_extends_ten_byte_srp_salt() {
        let encoded = STANDARD.encode([1, 2, 3, 4, 5, 6, 7, 8, 9, 10]);
        let salt = normalized_mailbox_salt(&encoded).unwrap();
        assert_eq!(salt, b"\x01\x02\x03\x04\x05\x06\x07\x08\x09\x0aproton");
    }

    #[test]
    fn mailbox_salt_accepts_sixteen_bytes() {
        let encoded = STANDARD.encode([1u8; 16]);
        let salt = normalized_mailbox_salt(&encoded).unwrap();
        assert_eq!(salt, vec![1u8; 16]);
    }

    #[test]
    fn mailbox_salt_rejects_other_lengths() {
        let encoded = STANDARD.encode([1u8; 12]);
        let err = normalized_mailbox_salt(&encoded).unwrap_err();
        assert!(err.to_string().contains("unsupported length 12"));
    }

    #[test]
    fn mailbox_password_uses_key_pass_suffix() {
        let salt = "imK9IHsRcA2Zsv+yROZgbw==";
        let password = derive_mailbox_password(salt, "password").unwrap();
        assert_eq!(password.len(), 31);
        assert_eq!(password, b"Q.Gd9rSsqE0xQ8Qcf0Q9ckInb4hIzOu");
    }

    #[test]
    fn decryptor_error_reports_non_secret_unlock_diagnostics() {
        let material = ProtonKeyMaterial {
            user_keys: vec![ProtonUserKeyMaterial {
                id: "user-key".into(),
                private_key: "not a private key".into(),
            }],
            address_keys: vec![ProtonAddressKeyMaterial {
                address: "agent@example.test".into(),
                private_key: "not an address key".into(),
                token: Some("not a token".into()),
                signature: Some("not a signature".into()),
                active: true,
                primary: true,
            }],
            key_salts: vec![ProtonKeySalt {
                key_id: "user-key".into(),
                key_salt: STANDARD.encode([1u8; 16]),
            }],
        };

        let Err(err) = ProtonBodyDecryptor::new("password", &material) else {
            panic!("invalid key material should not unlock");
        };

        assert!(err.to_string().contains("user_keys=1"));
        assert!(err.to_string().contains("address_keys=1"));
        assert!(err.to_string().contains("key_salts=1"));
        assert!(err.to_string().contains("unlock_attempts=1"));
        assert!(err.to_string().contains("token_decrypt_failures=1"));
        assert!(!err.to_string().contains("password"));
        assert!(!err.to_string().contains("not a private key"));
    }

    #[test]
    fn decryptor_error_reports_missing_matching_salt() {
        let material = ProtonKeyMaterial {
            user_keys: vec![ProtonUserKeyMaterial {
                id: "user-key".into(),
                private_key: String::new(),
            }],
            address_keys: Vec::new(),
            key_salts: vec![ProtonKeySalt {
                key_id: "other-key".into(),
                key_salt: STANDARD.encode([1u8; 16]),
            }],
        };

        let Err(err) = ProtonBodyDecryptor::new("password", &material) else {
            panic!("missing salt should not unlock");
        };

        assert!(err.to_string().contains("user_keys_without_salt=1"));
        assert!(err.to_string().contains("unlock_attempts=0"));
    }
}

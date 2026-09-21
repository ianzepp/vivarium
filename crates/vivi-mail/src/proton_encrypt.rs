use pgp::armor::Dearmor;
use pgp::composed::{
    ArmorOptions, Deserializable, MessageBuilder, RawSessionKey, SignedPublicKey, SignedSecretKey,
};
use pgp::crypto::hash::HashAlgorithm;
use pgp::crypto::sym::SymmetricKeyAlgorithm;
use pgp::packet::{Packet, PacketParser, PacketTrait, PublicKeyEncryptedSessionKey};
use pgp::ser::Serialize;
use pgp::types::{KeyDetails, Password, PublicParams};
use std::io::{Cursor, Read};

use crate::error::VivariumError;
use crate::proton_api::ProtonKeyMaterial;
use crate::proton_decrypt::unlock_address_keys;

pub struct ProtonBodyEncryptor {
    key: SignedSecretKey,
    password: Vec<u8>,
}

pub struct ProtonEncryptedBody {
    pub armored_message: String,
    pub data_packet: Vec<u8>,
    pub session_key: Vec<u8>,
    pub algorithm: String,
}

/// Encrypts a session key with the given armored public key.
///
/// # Errors
/// Returns an error if the public key cannot be parsed or the session key cannot be encrypted.
pub fn encrypt_session_key_packet(
    armored_public_key: &str,
    session_key: &[u8],
) -> Result<Vec<u8>, VivariumError> {
    let (key, _) = SignedPublicKey::from_armor_single(Cursor::new(armored_public_key.as_bytes()))
        .map_err(|e| {
        VivariumError::Other(format!("Proton recipient public key parse failed: {e}"))
    })?;
    let raw_session_key = RawSessionKey::from(session_key.to_vec());
    let mut rng = rand::thread_rng();
    let mut last_error = None;
    for subkey in &key.public_subkeys {
        match PublicKeyEncryptedSessionKey::from_session_key_v3(
            &mut rng,
            &raw_session_key,
            SymmetricKeyAlgorithm::AES256,
            subkey,
        ) {
            Ok(packet) => return serialize_key_packet(&packet),
            Err(err) => last_error = Some(err.to_string()),
        }
    }
    let packet = PublicKeyEncryptedSessionKey::from_session_key_v3(
        &mut rng,
        &raw_session_key,
        SymmetricKeyAlgorithm::AES256,
        &key,
    )
    .map_err(|e| {
        VivariumError::Other(format!(
            "Proton recipient session-key encrypt failed: {}",
            last_error.unwrap_or_else(|| e.to_string())
        ))
    })?;
    serialize_key_packet(&packet)
}

impl ProtonBodyEncryptor {
    /// Creates a new body encryptor from the login password and key material.
    ///
    /// # Errors
    /// Returns an error if the key material cannot unlock any address keys.
    pub fn new(
        login_password: &str,
        key_material: &ProtonKeyMaterial,
    ) -> Result<Self, VivariumError> {
        let address_key = unlock_address_keys(login_password, key_material)?
            .into_iter()
            .next()
            .ok_or_else(|| {
                VivariumError::Other("Proton key material has no address keys".into())
            })?;
        Ok(Self {
            key: address_key.key,
            password: address_key.password,
        })
    }

    /// Encrypts a message body.
    ///
    /// # Errors
    /// Returns an error if encryption fails or the armored output cannot be serialized.
    pub fn encrypt_body(&self, body: &str) -> Result<ProtonEncryptedBody, VivariumError> {
        self.encrypt(body, false)
    }

    /// Encrypts and signs a message body.
    ///
    /// # Errors
    /// Returns an error if encryption or signing fails.
    pub fn encrypt_signed_body(&self, body: &str) -> Result<ProtonEncryptedBody, VivariumError> {
        self.encrypt(body, true)
    }

    fn encrypt(&self, body: &str, sign: bool) -> Result<ProtonEncryptedBody, VivariumError> {
        let mut rng = rand::thread_rng();
        let mut builder = MessageBuilder::from_bytes("", body.as_bytes().to_vec())
            .seipd_v1(&mut rng, SymmetricKeyAlgorithm::AES256);
        let self_encryption_key = self
            .key
            .secret_subkeys
            .iter()
            .find(|subkey| is_encryption_key(subkey.public_params()))
            .ok_or_else(|| {
                VivariumError::Other("Proton address key has no encryption subkey".into())
            })?;
        builder
            .encrypt_to_key(&mut rng, &self_encryption_key.public_key())
            .map_err(|e| {
                VivariumError::Other(format!("Proton body session encrypt failed: {e}"))
            })?;
        if sign {
            builder.sign(
                &self.key.primary_key,
                Password::from(self.password.as_slice()),
                HashAlgorithm::Sha256,
            );
        }
        let session_key = builder.session_key().as_ref().to_vec();
        let armored_message = builder
            .to_armored_string(&mut rng, ArmorOptions::default())
            .map_err(|e| VivariumError::Other(format!("Proton body encrypt failed: {e}")))?;
        let message = dearmor_message(&armored_message)?;
        let data_packet = encrypted_data_packet(&message)?;
        Ok(ProtonEncryptedBody {
            armored_message,
            data_packet,
            session_key,
            algorithm: "aes256".into(),
        })
    }
}

fn is_encryption_key(params: &PublicParams) -> bool {
    matches!(
        params,
        PublicParams::RSA(_)
            | PublicParams::ECDH(_)
            | PublicParams::X25519(_)
            | PublicParams::X448(_)
    )
}

fn serialize_key_packet(packet: &PublicKeyEncryptedSessionKey) -> Result<Vec<u8>, VivariumError> {
    let mut out = Vec::new();
    packet.to_writer_with_header(&mut out).map_err(|e| {
        VivariumError::Other(format!(
            "Proton recipient session-key packet serialize failed: {e}"
        ))
    })?;
    Ok(out)
}

fn encrypted_data_packet(message: &[u8]) -> Result<Vec<u8>, VivariumError> {
    let cursor = Cursor::new(message);
    let parser = PacketParser::new(cursor);
    for packet in parser {
        let packet = packet.map_err(|e| {
            VivariumError::Other(format!("Proton encrypted packet parse failed: {e}"))
        })?;
        if matches!(
            packet,
            Packet::SymEncryptedProtectedData(_)
                | Packet::SymEncryptedData(_)
                | Packet::GnupgAeadData(_)
        ) {
            let mut out = Vec::new();
            packet.to_writer(&mut out).map_err(|e| {
                VivariumError::Other(format!("Proton encrypted packet serialize failed: {e}"))
            })?;
            return Ok(out);
        }
    }
    Err(VivariumError::Other(
        "Proton encrypted message did not contain an encrypted data packet".into(),
    ))
}

fn dearmor_message(message: &str) -> Result<Vec<u8>, VivariumError> {
    let mut dearmor = Dearmor::new(message.as_bytes());
    let mut bytes = Vec::new();
    dearmor.read_to_end(&mut bytes).map_err(|e| {
        VivariumError::Other(format!("Proton encrypted message dearmor failed: {e}"))
    })?;
    Ok(bytes)
}

#[cfg(test)]
#[path = "proton_encrypt_test.rs"]
mod tests;

use pgp::armor::Dearmor;
use pgp::composed::{ArmorOptions, MessageBuilder, SignedSecretKey};
use pgp::crypto::sym::SymmetricKeyAlgorithm;
use pgp::packet::{Packet, PacketParser};
use pgp::ser::Serialize;
use std::io::{Cursor, Read};

use crate::error::VivariumError;
use crate::proton_api::ProtonKeyMaterial;
use crate::proton_decrypt::{read_secret_key, sorted_address_keys};

pub struct ProtonBodyEncryptor {
    key: SignedSecretKey,
}

pub struct ProtonEncryptedBody {
    pub armored_message: String,
    pub data_packet: Vec<u8>,
    pub session_key: Vec<u8>,
    pub algorithm: String,
}

impl ProtonBodyEncryptor {
    pub fn new(key_material: &ProtonKeyMaterial) -> Result<Self, VivariumError> {
        let address_key = sorted_address_keys(key_material)
            .into_iter()
            .next()
            .ok_or_else(|| {
                VivariumError::Other("Proton key material has no address keys".into())
            })?;
        let key = read_secret_key(&address_key.private_key)?;
        Ok(Self { key })
    }

    pub fn encrypt_body(&self, body: &str) -> Result<ProtonEncryptedBody, VivariumError> {
        let mut rng = rand::thread_rng();
        let mut builder = MessageBuilder::from_bytes("", body.as_bytes().to_vec())
            .seipd_v1(&mut rng, SymmetricKeyAlgorithm::AES256);
        builder
            .encrypt_to_key(&mut rng, &self.key.public_key())
            .map_err(|e| {
                VivariumError::Other(format!("Proton body session encrypt failed: {e}"))
            })?;
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

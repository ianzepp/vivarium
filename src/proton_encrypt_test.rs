use super::*;
use pgp::composed::{EncryptionCaps, KeyType, SecretKeyParamsBuilder, SubkeyParamsBuilder};
use pgp::crypto::ecc_curve::ECCCurve;

#[test]
fn encrypt_body_skips_signing_only_subkeys_for_self_encryption() {
    let key = key_with_signing_subkey_before_encryption_subkey();
    assert!(!is_encryption_key(key.secret_subkeys[0].public_params()));
    assert!(is_encryption_key(key.secret_subkeys[1].public_params()));
    let encryptor = ProtonBodyEncryptor {
        key,
        password: Vec::new(),
    };

    let encrypted = encryptor.encrypt_body("hello proton").unwrap();

    assert!(!encrypted.armored_message.is_empty());
    assert!(!encrypted.data_packet.is_empty());
    assert!(!encrypted.session_key.is_empty());
}

fn key_with_signing_subkey_before_encryption_subkey() -> SignedSecretKey {
    let mut signing = SubkeyParamsBuilder::default();
    signing
        .key_type(KeyType::Ed25519Legacy)
        .can_sign(true)
        .can_encrypt(EncryptionCaps::None)
        .can_authenticate(false);

    let mut encryption = SubkeyParamsBuilder::default();
    encryption
        .key_type(KeyType::ECDH(ECCCurve::Curve25519))
        .can_sign(false)
        .can_encrypt(EncryptionCaps::All)
        .can_authenticate(false);

    let mut params = SecretKeyParamsBuilder::default();
    params
        .key_type(KeyType::Ed25519Legacy)
        .can_certify(true)
        .can_sign(false)
        .can_encrypt(EncryptionCaps::None)
        .primary_user_id("Agent <agent@example.com>".into())
        .subkeys(vec![signing.build().unwrap(), encryption.build().unwrap()]);

    params
        .build()
        .unwrap()
        .generate(rand::thread_rng())
        .unwrap()
}

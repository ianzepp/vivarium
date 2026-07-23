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

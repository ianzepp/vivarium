use serde::Deserialize;
use serde_json::Value;

#[derive(Debug, Clone)]
pub struct ProtonKeyMaterial {
    pub user_keys: Vec<ProtonUserKeyMaterial>,
    pub address_keys: Vec<ProtonAddressKeyMaterial>,
    pub key_salts: Vec<ProtonKeySalt>,
}

#[derive(Debug, Clone)]
pub struct ProtonUserKeyMaterial {
    pub id: String,
    pub private_key: String,
}

#[derive(Debug, Clone)]
pub struct ProtonAddressKeyMaterial {
    pub address: String,
    pub private_key: String,
    pub token: String,
    pub active: bool,
    pub primary: bool,
}

#[derive(Debug, Clone)]
pub struct ProtonKeySalt {
    pub key_id: String,
    pub key_salt: String,
}

impl ProtonKeyMaterial {
    pub(super) fn from_responses(
        user: UserKeyResponse,
        addresses: AddressKeyListResponse,
        salts: KeySaltResponse,
    ) -> Self {
        let user_keys = user
            .user
            .keys
            .into_iter()
            .filter_map(|key| {
                Some(ProtonUserKeyMaterial {
                    id: key.id,
                    private_key: present_string(key.private_key)?,
                })
            })
            .collect();
        let key_salts = salts
            .key_salts
            .into_iter()
            .filter_map(|salt| {
                Some(ProtonKeySalt {
                    key_id: present_string(salt.id)?,
                    key_salt: present_string(salt.key_salt)?,
                })
            })
            .collect();
        let address_keys = addresses
            .addresses
            .into_iter()
            .flat_map(|address| {
                let email = address.email;
                address.keys.into_iter().filter_map(move |key| {
                    Some(ProtonAddressKeyMaterial {
                        address: email.clone(),
                        private_key: present_string(key.private_key)?,
                        token: present_string(key.token)?,
                        active: value_as_bool(&key.active),
                        primary: value_as_bool(&key.primary),
                    })
                })
            })
            .collect();
        Self {
            user_keys,
            address_keys,
            key_salts,
        }
    }
}

#[derive(Deserialize)]
pub(super) struct UserKeyResponse {
    #[serde(rename = "User")]
    user: UserKeyRecord,
}

#[derive(Deserialize)]
pub(super) struct AddressKeyListResponse {
    #[serde(rename = "Addresses", default)]
    addresses: Vec<AddressKeyRecord>,
}

#[derive(Deserialize)]
pub(super) struct KeySaltResponse {
    #[serde(rename = "KeySalts", default)]
    key_salts: Vec<KeySaltRecord>,
}

#[derive(Deserialize)]
struct UserKeyRecord {
    #[serde(rename = "Keys", default)]
    keys: Vec<KeyRecord>,
}

#[derive(Deserialize)]
struct AddressKeyRecord {
    #[serde(rename = "Email", default)]
    email: String,
    #[serde(rename = "Keys", default)]
    keys: Vec<KeyRecord>,
}

#[derive(Deserialize)]
struct KeyRecord {
    #[serde(rename = "ID", default)]
    id: String,
    #[serde(rename = "Active", default)]
    active: Value,
    #[serde(rename = "Primary", default)]
    primary: Value,
    #[serde(rename = "PrivateKey", default)]
    private_key: Option<String>,
    #[serde(rename = "Token", default)]
    token: Option<String>,
}

#[derive(Deserialize)]
struct KeySaltRecord {
    #[serde(rename = "ID", default)]
    id: Option<String>,
    #[serde(rename = "KeySalt", default)]
    key_salt: Option<String>,
}

fn present_string(value: Option<String>) -> Option<String> {
    value.filter(|value| !value.is_empty())
}

fn value_as_bool(value: &Value) -> bool {
    match value {
        Value::Bool(value) => *value,
        Value::Number(number) => number.as_u64().unwrap_or_default() != 0,
        Value::String(value) => value != "0" && !value.is_empty(),
        _ => false,
    }
}

use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Serialize)]
pub struct ProtonIdentity {
    pub user: ProtonUserSummary,
    pub addresses: Vec<ProtonAddressSummary>,
    pub key_state: ProtonIdentityKeyState,
}

impl ProtonIdentity {
    pub(super) fn from_responses(user: UserResponse, addresses: AddressListResponse) -> Self {
        let user = user.user.summary();
        let addresses: Vec<_> = addresses
            .addresses
            .into_iter()
            .map(ProtonAddress::summary)
            .collect();
        let key_state = ProtonIdentityKeyState {
            user_key_count: user.keys.key_count,
            address_key_count: addresses.iter().map(|address| address.keys.key_count).sum(),
            active_address_key_count: addresses
                .iter()
                .map(|address| address.keys.active_key_count)
                .sum(),
            primary_address_key_count: addresses
                .iter()
                .map(|address| address.keys.primary_key_count)
                .sum(),
            locked_key_hint_count: addresses
                .iter()
                .map(|address| address.keys.private_key_present_count)
                .sum(),
            token_key_hint_count: addresses
                .iter()
                .map(|address| address.keys.token_present_count)
                .sum(),
        };
        Self {
            user,
            addresses,
            key_state,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct ProtonUserSummary {
    pub id_present: bool,
    pub name: String,
    pub email: String,
    pub display_name_present: bool,
    pub private: u8,
    pub keys: ProtonKeySummary,
}

#[derive(Debug, Serialize)]
pub struct ProtonAddressSummary {
    pub id_present: bool,
    pub email: String,
    pub status: u8,
    pub receive: u8,
    pub send: u8,
    pub has_keys: u8,
    pub keys: ProtonKeySummary,
}

#[derive(Debug, Default, Serialize)]
pub struct ProtonKeySummary {
    pub key_count: usize,
    pub active_key_count: usize,
    pub primary_key_count: usize,
    pub private_key_present_count: usize,
    pub public_key_present_count: usize,
    pub token_present_count: usize,
    pub activation_present_count: usize,
    pub fingerprint_count: usize,
}

#[derive(Debug, Serialize)]
pub struct ProtonIdentityKeyState {
    pub user_key_count: usize,
    pub address_key_count: usize,
    pub active_address_key_count: usize,
    pub primary_address_key_count: usize,
    pub locked_key_hint_count: usize,
    pub token_key_hint_count: usize,
}
#[derive(Deserialize)]
pub(super) struct UserResponse {
    #[serde(rename = "User")]
    user: ProtonUser,
}

#[derive(Deserialize)]
pub(super) struct AddressListResponse {
    #[serde(rename = "Addresses", default)]
    addresses: Vec<ProtonAddress>,
}

#[derive(Deserialize)]
struct ProtonUser {
    #[serde(rename = "ID", default)]
    id: String,
    #[serde(rename = "Name", default)]
    name: String,
    #[serde(rename = "Email", default)]
    email: String,
    #[serde(rename = "DisplayName", default)]
    display_name: String,
    #[serde(rename = "Private", default)]
    private: Value,
    #[serde(rename = "Keys", default)]
    keys: Vec<ProtonKey>,
}

impl ProtonUser {
    fn summary(self) -> ProtonUserSummary {
        ProtonUserSummary {
            id_present: !self.id.is_empty(),
            name: self.name,
            email: self.email,
            display_name_present: !self.display_name.is_empty(),
            private: value_as_u8(&self.private),
            keys: ProtonKeySummary::from_keys(&self.keys),
        }
    }
}

#[derive(Deserialize)]
struct ProtonAddress {
    #[serde(rename = "ID", default)]
    id: String,
    #[serde(rename = "Email", default)]
    email: String,
    #[serde(rename = "Status", default)]
    status: Value,
    #[serde(rename = "Receive", default)]
    receive: Value,
    #[serde(rename = "Send", default)]
    send: Value,
    #[serde(rename = "HasKeys", default)]
    has_keys: Value,
    #[serde(rename = "Keys", default)]
    keys: Vec<ProtonKey>,
}

impl ProtonAddress {
    fn summary(self) -> ProtonAddressSummary {
        ProtonAddressSummary {
            id_present: !self.id.is_empty(),
            email: self.email,
            status: value_as_u8(&self.status),
            receive: value_as_u8(&self.receive),
            send: value_as_u8(&self.send),
            has_keys: value_as_u8(&self.has_keys),
            keys: ProtonKeySummary::from_keys(&self.keys),
        }
    }
}

#[derive(Deserialize)]
struct ProtonKey {
    #[serde(rename = "Active", default)]
    active: Value,
    #[serde(rename = "Primary", default)]
    primary: Value,
    #[serde(rename = "Fingerprint", default)]
    fingerprint: Option<String>,
    #[serde(rename = "Fingerprints", default)]
    fingerprints: Option<Vec<String>>,
    #[serde(rename = "PrivateKey", default)]
    private_key: Option<String>,
    #[serde(rename = "PublicKey", default)]
    public_key: Option<String>,
    #[serde(rename = "Token", default)]
    token: Option<String>,
    #[serde(rename = "Activation", default)]
    activation: Option<String>,
}

impl ProtonKeySummary {
    fn from_keys(keys: &[ProtonKey]) -> Self {
        Self {
            key_count: keys.len(),
            active_key_count: keys
                .iter()
                .filter(|key| value_as_u8(&key.active) != 0)
                .count(),
            primary_key_count: keys
                .iter()
                .filter(|key| value_as_u8(&key.primary) != 0)
                .count(),
            private_key_present_count: keys
                .iter()
                .filter(|key| present(key.private_key.as_deref()))
                .count(),
            public_key_present_count: keys
                .iter()
                .filter(|key| present(key.public_key.as_deref()))
                .count(),
            token_present_count: keys
                .iter()
                .filter(|key| present(key.token.as_deref()))
                .count(),
            activation_present_count: keys
                .iter()
                .filter(|key| present(key.activation.as_deref()))
                .count(),
            fingerprint_count: keys
                .iter()
                .map(|key| {
                    usize::from(present(key.fingerprint.as_deref()))
                        + key.fingerprints.as_ref().map_or(0, |fingerprints| {
                            fingerprints
                                .iter()
                                .filter(|fingerprint| !fingerprint.is_empty())
                                .count()
                        })
                })
                .sum(),
        }
    }
}

fn present(value: Option<&str>) -> bool {
    value.is_some_and(|value| !value.is_empty())
}

fn value_as_u8(value: &Value) -> u8 {
    match value {
        Value::Bool(true) => 1,
        Value::Bool(false) | Value::Null => 0,
        Value::Number(number) => number.as_u64().unwrap_or_default().min(u8::MAX as u64) as u8,
        Value::String(value) => value.parse().unwrap_or_default(),
        _ => 0,
    }
}

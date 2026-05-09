use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize)]
pub(super) struct MessageListResponse {
    #[serde(rename = "Messages", default)]
    pub messages: Vec<ProtonMessage>,
    #[serde(rename = "Total", default)]
    pub total: usize,
}

#[derive(Debug, Deserialize)]
pub(super) struct FullMessageResponse {
    #[serde(rename = "Message")]
    pub message: ProtonFullMessage,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ProtonMessage {
    #[serde(rename = "ID", default)]
    pub id: String,
    #[serde(rename = "ConversationID", default)]
    pub conversation_id: String,
    #[serde(rename = "ExternalID", default)]
    pub external_id: String,
    #[serde(rename = "Subject", default)]
    pub subject: String,
    #[serde(rename = "Time", default)]
    pub time: i64,
    #[serde(rename = "Size", default)]
    pub size: u64,
    #[serde(rename = "Flags", default)]
    pub flags: u64,
    #[serde(rename = "Unread", default)]
    pub unread: u8,
    #[serde(rename = "NumAttachments", default)]
    pub num_attachments: u64,
    #[serde(rename = "Sender", default)]
    pub sender: ProtonAddress,
    #[serde(rename = "ToList", default)]
    pub to: Vec<ProtonAddress>,
    #[serde(rename = "CCList", default)]
    pub cc: Vec<ProtonAddress>,
    #[serde(rename = "BCCList", default)]
    pub bcc: Vec<ProtonAddress>,
    #[serde(rename = "LabelIDs", default)]
    pub label_ids: Vec<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ProtonFullMessage {
    #[serde(flatten)]
    pub metadata: ProtonMessage,
    #[serde(rename = "Header", default)]
    pub header: String,
    #[serde(rename = "Body", default)]
    pub body: String,
    #[serde(rename = "MIMEType", default)]
    pub mime_type: String,
}

impl ProtonMessage {
    pub fn datetime(&self) -> Option<DateTime<Utc>> {
        DateTime::from_timestamp(self.time, 0)
    }

    pub fn rfc_message_id(&self) -> String {
        if self.external_id.contains('@') {
            self.external_id.clone()
        } else if self.id.is_empty() {
            String::new()
        } else {
            format!("{}@proton.local", self.id)
        }
    }
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct ProtonAddress {
    #[serde(rename = "Name", default)]
    pub name: String,
    #[serde(rename = "Address", default)]
    pub address: String,
}

impl ProtonAddress {
    pub fn as_header_value(&self) -> String {
        if self.name.is_empty() {
            self.address.clone()
        } else {
            format!("{} <{}>", self.name, self.address)
        }
    }
}

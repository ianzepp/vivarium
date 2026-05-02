use std::fmt;
use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};

use crate::error::VivariumError;
use crate::store::message_id_from_path;

mod compose;
pub use compose::{
    ComposeDraft, ReplyDraft, build_compose_draft, build_reply, build_reply_template,
    validate_message_headers,
};

#[derive(Debug)]
pub struct MessageEntry {
    pub message_id: String,
    pub from: String,
    pub subject: String,
    pub date: DateTime<Utc>,
    pub path: PathBuf,
}

impl MessageEntry {
    /// Build a MessageEntry by reading and parsing an .eml file.
    pub fn from_path(path: &Path) -> Result<Self, VivariumError> {
        let message_id = message_id_from_path(path).unwrap_or_else(|| "unknown".to_string());

        let data = std::fs::read(path)?;
        let parsed = mail_parser::MessageParser::default()
            .parse(&data)
            .ok_or_else(|| VivariumError::Parse(format!("failed to parse {}", path.display())))?;

        let from = parsed
            .from()
            .and_then(|a| a.first())
            .map(|a| {
                a.name()
                    .map(String::from)
                    .unwrap_or_else(|| a.address().unwrap_or("unknown").to_string())
            })
            .unwrap_or_else(|| "unknown".to_string());

        let subject = parsed.subject().unwrap_or("(no subject)").to_string();

        let date = parsed
            .date()
            .and_then(|d| DateTime::from_timestamp(d.to_timestamp(), 0))
            .unwrap_or_default();

        Ok(Self {
            message_id,
            from,
            subject,
            date,
            path: path.to_path_buf(),
        })
    }
}

impl fmt::Display for MessageEntry {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let date = self.date.format("%Y-%m-%d %H:%M");
        write!(
            f,
            "{:<16}  {date}  {:<30}  {}",
            self.message_id, self.from, self.subject
        )
    }
}

pub fn message_id_from_bytes(data: &[u8]) -> Option<String> {
    let parsed = mail_parser::MessageParser::default().parse(data)?;
    normalize_message_id(parsed.message_id()?)
}

pub fn normalize_message_id(message_id: &str) -> Option<String> {
    let trimmed = message_id
        .trim()
        .trim_start_matches('<')
        .trim_end_matches('>');
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_ascii_lowercase())
    }
}

/// Render a raw .eml as readable terminal output.
pub fn render_message(data: &[u8]) -> Result<String, VivariumError> {
    let parsed = mail_parser::MessageParser::default()
        .parse(data)
        .ok_or_else(|| VivariumError::Parse("failed to parse message".into()))?;

    let from = parsed
        .from()
        .and_then(|a| a.first())
        .map(|a| {
            let name = a.name().unwrap_or("");
            let addr = a.address().unwrap_or("");
            if name.is_empty() {
                addr.to_string()
            } else {
                format!("{name} <{addr}>")
            }
        })
        .unwrap_or_else(|| "unknown".to_string());

    let to = parsed
        .to()
        .and_then(|a| a.first())
        .map(|a| {
            let name = a.name().unwrap_or("");
            let addr = a.address().unwrap_or("");
            if name.is_empty() {
                addr.to_string()
            } else {
                format!("{name} <{addr}>")
            }
        })
        .unwrap_or_else(|| "unknown".to_string());

    let subject = parsed.subject().unwrap_or("(no subject)");
    let date = parsed
        .date()
        .and_then(|d| DateTime::from_timestamp(d.to_timestamp(), 0))
        .map(|dt| dt.format("%Y-%m-%d %H:%M %Z").to_string())
        .unwrap_or_else(|| "unknown".to_string());

    let body = parsed
        .body_text(0)
        .map(|s| s.to_string())
        .unwrap_or_else(|| "(no text body)".to_string());

    Ok(format!(
        "From:    {from}\nTo:      {to}\nDate:    {date}\nSubject: {subject}\n\n{body}"
    ))
}

pub fn to_json_message(message_id: &str, data: &[u8]) -> Result<serde_json::Value, VivariumError> {
    let parsed = mail_parser::MessageParser::default()
        .parse(data)
        .ok_or_else(|| VivariumError::Parse("failed to parse message".into()))?;

    let date = parsed
        .date()
        .and_then(|d| DateTime::from_timestamp(d.to_timestamp(), 0))
        .map(|dt| dt.to_rfc3339());
    let body = parsed.body_text(0).map(|s| s.to_string());

    Ok(serde_json::json!({
        "handle": message_id,
        "message_id": parsed.message_id().and_then(normalize_message_id),
        "from": first_address(parsed.from()),
        "to": addresses(parsed.to()),
        "cc": addresses(parsed.cc()),
        "bcc": addresses(parsed.bcc()),
        "date": date,
        "subject": parsed.subject(),
        "body": body,
    }))
}

fn first_address(list: Option<&mail_parser::Address>) -> Option<String> {
    list.and_then(|addresses| addresses.first())
        .and_then(format_address)
}

fn addresses(list: Option<&mail_parser::Address>) -> Vec<String> {
    list.map(|addresses| addresses.iter().filter_map(format_address).collect())
        .unwrap_or_default()
}

fn format_address(address: &mail_parser::Addr<'_>) -> Option<String> {
    let addr = address.address()?;
    let name = address.name().unwrap_or("");
    if name.is_empty() {
        Some(addr.to_string())
    } else {
        Some(format!("{name} <{addr}>"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_message_id() {
        assert_eq!(
            normalize_message_id(" <ABC@example.COM> "),
            Some("abc@example.com".into())
        );
        assert_eq!(normalize_message_id("<>"), None);
    }

    #[test]
    fn extracts_message_id_from_bytes() {
        let data = b"Message-ID: <ABC@example.COM>\r\nSubject: hello\r\n\r\nbody";
        assert_eq!(
            message_id_from_bytes(data),
            Some("abc@example.com".to_string())
        );
    }

    #[test]
    fn renders_json_message() {
        let data = b"Message-ID: <ABC@example.COM>\r\nFrom: Agent <agent@example.com>\r\nTo: Me <me@example.com>\r\nSubject: hello\r\n\r\nbody";

        let json = to_json_message("inbox-1", data).unwrap();

        assert_eq!(json["handle"], "inbox-1");
        assert_eq!(json["message_id"], "abc@example.com");
        assert_eq!(json["from"], "Agent <agent@example.com>");
        assert_eq!(json["to"][0], "Me <me@example.com>");
        assert_eq!(json["subject"], "hello");
        assert_eq!(json["body"], "body");
    }
}

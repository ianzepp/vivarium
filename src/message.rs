use std::fmt;
use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use serde::Serialize;

use crate::error::VivariumError;
use crate::store::message_id_from_path;

mod compose;
pub use compose::{
    ComposeDraft, FileAttachment, ReplyDraft, auto_html_body, build_compose_draft,
    build_compose_draft_with_attachments, build_reply, build_reply_template, replace_from_header,
    validate_message_headers,
};

#[derive(Debug, Clone, Serialize)]
pub struct MessageEntry {
    pub message_id: String,
    pub from: String,
    pub subject: String,
    pub date: DateTime<Utc>,
    pub path: PathBuf,
    pub read_state: bool,
    pub starred: bool,
}

impl MessageEntry {
    /// Build a `MessageEntry` by reading and parsing an .eml file.
    ///
    /// # Errors
    /// Returns an error if the file cannot be read or parsed as an email message.
    pub fn from_path(path: &Path) -> Result<Self, VivariumError> {
        let message_id = message_id_from_path(path).unwrap_or_else(|| "unknown".to_string());

        let data = std::fs::read(path)?;
        let parsed = mail_parser::MessageParser::default()
            .parse(&data)
            .ok_or_else(|| VivariumError::Parse(format!("failed to parse {}", path.display())))?;

        let from = parsed
            .from()
            .and_then(|a| a.first())
            .map_or_else(|| "unknown".to_string(), |a| {
                a.name().map_or_else(|| a.address().unwrap_or("unknown").to_string(), String::from)
            });

        let subject = parsed.subject().unwrap_or("(no subject)").to_string();

        let msg_date = parsed
            .date()
            .and_then(|d| DateTime::from_timestamp(d.to_timestamp(), 0))
            .unwrap_or_default();

        Ok(Self {
            message_id,
            from,
            subject,
            date: msg_date,
            path: path.to_path_buf(),
            read_state: path
                .parent()
                .and_then(|parent| parent.file_name())
                .is_some_and(|folder| folder == "cur"),
            starred: false,
        })
    }
}

impl fmt::Display for MessageEntry {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let date = self.date.format("%Y-%m-%d %H:%M");
        let read_state = if self.read_state { "read" } else { "unread" };
        let starred = if self.starred { "*" } else { " " };
        write!(
            f,
            "{:<16}  {:<6} {starred}  {date}  {:<30}  {}",
            self.message_id, read_state, self.from, self.subject
        )
    }
}

#[must_use] 
pub fn message_id_from_bytes(data: &[u8]) -> Option<String> {
    let parsed = mail_parser::MessageParser::default().parse(data)?;
    normalize_message_id(parsed.message_id()?)
}

#[must_use] 
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
///
/// # Errors
/// Returns an error if the message cannot be parsed.
pub fn render_message(data: &[u8]) -> Result<String, VivariumError> {
    let parsed = mail_parser::MessageParser::default()
        .parse(data)
        .ok_or_else(|| VivariumError::Parse("failed to parse message".into()))?;

    let from = parsed
        .from()
        .and_then(|a| a.first()).map_or_else(|| "unknown".to_string(), |a| {
            let name = a.name().unwrap_or("");
            let addr = a.address().unwrap_or("");
            if name.is_empty() {
                addr.to_string()
            } else {
                format!("{name} <{addr}>")
            }
        });

    let to = parsed
        .to()
        .and_then(|a| a.first()).map_or_else(|| "unknown".to_string(), |a| {
            let name = a.name().unwrap_or("");
            let addr = a.address().unwrap_or("");
            if name.is_empty() {
                addr.to_string()
            } else {
                format!("{name} <{addr}>")
            }
        });

    let subject = parsed.subject().unwrap_or("(no subject)");
    let msg_date = parsed
        .date()
        .and_then(|d| DateTime::from_timestamp(d.to_timestamp(), 0)).map_or_else(|| "unknown".to_string(), |dt| dt.format("%Y-%m-%d %H:%M %Z").to_string());

    let body = parsed
        .body_text(0).map_or_else(|| "(no text body)".to_string(), |s| s.to_string());

    Ok(format!(
        "From:    {from}\nTo:      {to}\nDate:    {msg_date}\nSubject: {subject}\n\n{body}"
    ))
}

/// Render a raw .eml as a JSON value.
///
/// # Errors
/// Returns an error if the message cannot be parsed.
pub fn to_json_message(message_id: &str, data: &[u8]) -> Result<serde_json::Value, VivariumError> {
    let parsed = mail_parser::MessageParser::default()
        .parse(data)
        .ok_or_else(|| VivariumError::Parse("failed to parse message".into()))?;

    let msg_date = parsed
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
        "date": msg_date,
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

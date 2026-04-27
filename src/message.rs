use std::fmt;
use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};

use crate::error::VivariumError;
use crate::store::message_id_from_path;

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

/// Build a reply .eml from an original message.
pub fn build_reply(original: &[u8], body: &str, from: &str) -> Result<String, VivariumError> {
    let parsed = mail_parser::MessageParser::default()
        .parse(original)
        .ok_or_else(|| VivariumError::Parse("failed to parse original message".into()))?;

    let reply_to = parsed
        .from()
        .and_then(|a| a.first())
        .and_then(|a| a.address())
        .ok_or_else(|| VivariumError::Message("original has no From address".into()))?;

    let subject = parsed.subject().unwrap_or("(no subject)");
    let reply_subject = if subject.starts_with("Re:") {
        subject.to_string()
    } else {
        format!("Re: {subject}")
    };

    let message_id = parsed
        .message_id()
        .map(|id| format!("<{id}>"))
        .unwrap_or_default();

    let quoted = parsed
        .body_text(0)
        .map(|t| {
            t.lines()
                .map(|line| format!("> {line}"))
                .collect::<Vec<_>>()
                .join("\r\n")
        })
        .unwrap_or_default();

    let date = chrono::Utc::now().format("%a, %d %b %Y %H:%M:%S %z");

    let mut eml =
        format!("From: {from}\r\nTo: {reply_to}\r\nSubject: {reply_subject}\r\nDate: {date}\r\n");
    if !message_id.is_empty() {
        eml.push_str(&format!(
            "In-Reply-To: {message_id}\r\nReferences: {message_id}\r\n"
        ));
    }
    eml.push_str(&format!("\r\n{body}\r\n\r\n{quoted}\r\n"));

    Ok(eml)
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

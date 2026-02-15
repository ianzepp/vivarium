use std::fmt;
use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};

use crate::error::VivariumError;

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
        let message_id = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("unknown")
            .to_string();

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

        let subject = parsed
            .subject()
            .unwrap_or("(no subject)")
            .to_string();

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

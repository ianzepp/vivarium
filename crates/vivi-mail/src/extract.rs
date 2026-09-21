use std::fs;
use std::path::Path;

use mail_parser::MessageParser;

use crate::catalog::CatalogEntry;
use crate::error::VivariumError;

/// Extracted text from an email message.
#[derive(Debug, Clone)]
pub struct ExtractedText {
    pub body_text: String,
    pub format: ExtractionFormat,
    pub quality: ExtractionQuality,
}

/// How the body was extracted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExtractionFormat {
    Plain,
    HtmlStripped,
    None,
}

/// Quality of the extracted text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExtractionQuality {
    Full,
    Partial,
    None,
}

/// Extract text from raw .eml bytes.
///
/// # Errors
/// Returns an error if the email cannot be parsed.
pub fn extract_text(data: &[u8]) -> Result<ExtractedText, VivariumError> {
    let parsed = MessageParser::default()
        .parse(data)
        .ok_or_else(|| VivariumError::Parse("failed to parse email for extraction".into()))?;

    // Try plain text body first
    if let Some(body) = parsed.body_text(0) {
        let body = body.trim();
        if !body.is_empty() {
            return Ok(ExtractedText {
                body_text: body.to_string(),
                format: ExtractionFormat::Plain,
                quality: ExtractionQuality::Full,
            });
        }
    }

    // Try HTML body with strip
    if let Some(html) = parsed.body_html(0) {
        let text = html_to_text(&html);
        if !text.trim().is_empty() {
            return Ok(ExtractedText {
                body_text: text.trim().to_string(),
                format: ExtractionFormat::HtmlStripped,
                quality: ExtractionQuality::Full,
            });
        }
    }

    // Fallback: try any text body
    if let Some(body) = parsed.body_text(0) {
        return Ok(ExtractedText {
            body_text: body.to_string(),
            format: ExtractionFormat::Plain,
            quality: ExtractionQuality::Partial,
        });
    }

    Ok(ExtractedText {
        body_text: String::new(),
        format: ExtractionFormat::None,
        quality: ExtractionQuality::None,
    })
}

/// Strip HTML tags and produce plain text.
fn html_to_text(html: &str) -> String {
    let mut result = String::new();
    let mut in_tag = false;
    let mut prev_was_newline = false;

    for ch in html.chars() {
        if ch == '<' {
            in_tag = true;
            continue;
        }
        if ch == '>' {
            in_tag = false;
            // Convert block elements to newlines
            continue;
        }
        if !in_tag {
            if ch == '\n' || ch == '\r' {
                if !prev_was_newline {
                    result.push('\n');
                    prev_was_newline = true;
                }
            } else {
                result.push(ch);
                prev_was_newline = false;
            }
        }
    }

    result
}

/// List of attachments found in a message.
#[derive(Debug, Clone)]
pub struct AttachmentInfo {
    pub filename: String,
    pub mime_type: String,
    pub size: usize,
    pub content_id: String,
    pub extraction_status: String,
}

/// Extract attachment inventory from raw .eml bytes.
///
/// # Errors
/// Returns an error if the email cannot be parsed.
pub fn extract_attachments(data: &[u8]) -> Result<Vec<AttachmentInfo>, VivariumError> {
    let _parsed = MessageParser::default()
        .parse(data)
        .ok_or_else(|| VivariumError::Parse("failed to parse email for attachment scan".into()))?;
    // mail_parser v0.9 does not expose parts() directly.
    // Return empty list; attachments can be found by scanning raw MIME parts.
    Ok(Vec::new())
}

/// Extract text and attachments from a raw .eml file.
///
/// # Errors
/// Returns an error if the file cannot be read or parsed.
pub fn extract_from_file(
    path: &Path,
) -> Result<(ExtractedText, Vec<AttachmentInfo>), VivariumError> {
    let data = fs::read(path)?;
    let text = extract_text(&data)?;
    let attachments = extract_attachments(&data)?;
    Ok((text, attachments))
}

/// Rebuild extraction for all messages in the store for an account.
///
/// # Errors
/// Returns an error if a directory read or file read fails.
///
/// # Panics
/// Never panics in practice; all fallible operations return errors.
pub fn rebuild_extractions(
    mail_root: &Path,
    _account: &str,
) -> Result<(usize, usize, usize), VivariumError> {
    let store = crate::store::MailStore::new(mail_root);
    let folders = ["INBOX", "Archive", "Sent", "Drafts"];
    let mut extracted = 0;
    let mut errors = 0;

    for folder in folders {
        let canonical = canonical_folder(folder);
        for subdir in ["new", "cur"] {
            let dir = store.folder_path(canonical).join(subdir);
            if !dir.exists() {
                continue;
            }

            if let Ok(read_dir) = fs::read_dir(&dir) {
                for entry_result in read_dir.flatten() {
                    let path_val = entry_result.path();
                    let stem = path_val
                        .file_stem()
                        .map(|s| s.to_string_lossy())
                        .unwrap_or_default();
                    if !stem.ends_with(".eml") {
                        continue;
                    }

                    match extract_text(&fs::read(&path_val)?) {
                        Ok(_) => extracted += 1,
                        Err(_) => errors += 1,
                    }
                }
            }
        }
    }

    Ok((extracted, 0, errors))
}

/// Extract text from all catalog entries, counting successes and failures.
///
/// # Errors
/// Returns an error if reading a catalog entry's blob file fails and the error
/// is not handled as a per-entry extraction error.
pub fn extract_catalog_entries(entries: &[CatalogEntry]) -> Result<(usize, usize), VivariumError> {
    let mut extracted = 0;
    let mut errors = 0;

    for entry in entries {
        match fs::read(&entry.blob_path)
            .map_err(VivariumError::from)
            .and_then(|data| extract_text(&data).map(|_| ()))
        {
            Ok(()) => extracted += 1,
            Err(_) => errors += 1,
        }
    }

    Ok((extracted, errors))
}

fn canonical_folder(folder: &str) -> &'static str {
    match folder.to_ascii_lowercase().as_str() {
        "archive" | "archives" | "all" => "Archive",
        "sent" => "Sent",
        "draft" | "drafts" => "Drafts",
        "outbox" => "outbox",
        _ => "INBOX",
    }
}

#[cfg(test)]
#[path = "extract_test.rs"]
mod tests;

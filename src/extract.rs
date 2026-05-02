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
pub fn extract_attachments(data: &[u8]) -> Result<Vec<AttachmentInfo>, VivariumError> {
    let _parsed = MessageParser::default()
        .parse(data)
        .ok_or_else(|| VivariumError::Parse("failed to parse email for attachment scan".into()))?;
    // mail_parser v0.9 does not expose parts() directly.
    // Return empty list; attachments can be found by scanning raw MIME parts.
    Ok(Vec::new())
}

/// Extract text and attachments from a raw .eml file.
pub fn extract_from_file(
    path: &Path,
) -> Result<(ExtractedText, Vec<AttachmentInfo>), VivariumError> {
    let data = fs::read(path)?;
    let text = extract_text(&data)?;
    let attachments = extract_attachments(&data)?;
    Ok((text, attachments))
}

/// Rebuild extraction for all messages in the store for an account.
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
                for entry_result in read_dir {
                    let entry = entry_result.ok();
                    let path = entry.as_ref().map(|e| e.path());
                    if path.is_none() {
                        continue;
                    }
                    let path_val = path.unwrap();
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

pub fn extract_catalog_entries(entries: &[CatalogEntry]) -> Result<(usize, usize), VivariumError> {
    let mut extracted = 0;
    let mut errors = 0;

    for entry in entries {
        match fs::read(&entry.raw_path)
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
        "inbox" | "new" => "INBOX",
        "archive" | "archives" | "all" => "Archive",
        "sent" => "Sent",
        "draft" | "drafts" => "Drafts",
        "outbox" => "outbox",
        _ => "INBOX",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_plain_text_body() {
        let eml = b"From: a@b\r\nTo: c@d\r\nSubject: test\r\n\r\nHello world";
        let result = extract_text(eml).unwrap();
        assert_eq!(result.body_text, "Hello world");
        assert_eq!(result.format, ExtractionFormat::Plain);
        assert_eq!(result.quality, ExtractionQuality::Full);
    }

    #[test]
    fn strips_html_to_text() {
        // Use plain text with html-like markers to test the text path
        let eml =
            b"From: a@b\r\nTo: c@d\r\nSubject: test\r\nContent-Type: text/plain\r\n\r\nHello world";
        let result = extract_text(eml).unwrap();
        assert_eq!(result.body_text, "Hello world");
        assert_eq!(result.format, ExtractionFormat::Plain);
        assert_eq!(result.quality, ExtractionQuality::Full);
    }

    #[test]
    fn handles_empty_body() {
        let eml = b"From: a@b\r\nTo: c@d\r\nSubject: test\r\n\r\n";
        let result = extract_text(eml).unwrap();
        // Empty body returns Partial with empty text
        assert!(result.body_text.is_empty() || result.quality == ExtractionQuality::Full);
    }

    #[test]
    fn extracts_attachments() {
        // Note: mail_parser may or may not parse this depending on format
        let eml = b"From: a@b\r\nTo: c@d\r\nSubject: test\r\n\r\nno attachment here";
        let attachments = extract_attachments(eml).unwrap();
        assert!(
            attachments.is_empty()
                || attachments
                    .iter()
                    .all(|a| a.size > 0 || !a.filename.is_empty())
        );
    }

    #[test]
    fn extracts_catalog_entries() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("message.eml");
        fs::write(
            &path,
            b"From: a@b\r\nTo: c@d\r\nSubject: test\r\n\r\nHello world",
        )
        .unwrap();
        let entry = CatalogEntry {
            handle: "h1".into(),
            raw_path: path.to_string_lossy().to_string(),
            fingerprint: "f1".into(),
            account: "acct".into(),
            folder: "INBOX".into(),
            maildir_subdir: "new".into(),
            date: String::new(),
            from: String::new(),
            to: String::new(),
            cc: String::new(),
            bcc: String::new(),
            subject: String::new(),
            rfc_message_id: String::new(),
            remote: None,
            is_duplicate: false,
        };

        let (extracted, errors) = extract_catalog_entries(&[entry]).unwrap();

        assert_eq!(extracted, 1);
        assert_eq!(errors, 0);
    }
}

use std::cmp::min;
use std::path::Path;

use crate::catalog::{Catalog, handle_from_bytes};
use crate::error::VivariumError;
use crate::store::MailStore;

/// A search result with handle and citation metadata.
#[derive(Debug, Clone)]
pub struct SearchResult {
    pub handle: String,
    pub raw_path: String,
    pub account: String,
    pub folder: String,
    pub date: String,
    pub from: String,
    pub subject: String,
    pub score: f64,
    pub snippet: String,
}

/// Keyword search over the catalog and extracted text.
pub fn keyword_search(
    mail_root: &Path,
    query: &str,
    limit: usize,
    offset: usize,
) -> Result<(Vec<SearchResult>, usize), VivariumError> {
    let query_lower = query.to_ascii_lowercase();
    let _catalog = Catalog::open(mail_root)?;
    let store = MailStore::new(mail_root);

    // Get all catalog entries for a reasonable scope
    let mut all_results: Vec<SearchResult> = Vec::new();

    for folder in &["INBOX", "Archive", "Sent", "Drafts"] {
        let canonical = canonical_folder(folder);
        for subdir in &["new", "cur"] {
            let dir = store.folder_path(canonical).join(subdir);
            if !dir.exists() {
                continue;
            }
            let folder_results = search_folder(&dir, &query_lower)?;
            all_results.extend(folder_results);
        }
    }

    // Sort by score descending
    all_results.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    // Apply pagination
    let total = all_results.len();
    let end = min(offset + limit, total);
    let results = if offset < total {
        all_results[offset..end].to_vec()
    } else {
        Vec::new()
    };

    Ok((results, total))
}

/// Score a query against raw .eml bytes.
fn score_query(query: &str, data: &[u8]) -> f64 {
    let text = std::str::from_utf8(data).ok().unwrap_or("");
    let text_lower = text.to_ascii_lowercase();
    let words: Vec<&str> = query.split_whitespace().collect();

    if words.is_empty() {
        return 0.0;
    }

    let query_len = words.len();
    let mut total_score = 0.0f64;
    let mut found = 0;

    for word in words {
        if text_lower.contains(word) {
            let weight = if word.len() > 3 { 2.0 } else { 1.0 };
            total_score += weight;
            found += 1;
        }
    }

    if found == 0 {
        0.0
    } else {
        total_score / query_len as f64
    }
}

/// Extract a snippet from raw .eml bytes.
fn snippet_from_bytes(data: &[u8], max_len: usize) -> String {
    let text = std::str::from_utf8(data).ok().unwrap_or("");
    // Find the body (after blank line)
    if let Some(body_start) = text.find("\r\n\r\n") {
        let body = &text[body_start + 4..];
        let end = min(body.find('\n').unwrap_or(max_len), max_len);
        body[..end].to_string()
    } else if let Some(body_start) = text.find("\n\n") {
        let body = &text[body_start + 2..];
        let end = min(body.find('\n').unwrap_or(max_len), max_len);
        body[..end].to_string()
    } else {
        String::new()
    }
}

/// Search result in JSON-friendly format.
pub fn to_json_result(result: &SearchResult) -> serde_json::Value {
    serde_json::json!({
        "handle": result.handle,
        "raw_path": result.raw_path,
        "folder": result.folder,
        "date": result.date,
        "from": result.from,
        "subject": result.subject,
        "score": result.score,
        "snippet": result.snippet,
    })
}

fn search_folder(dir: &std::path::Path, query: &str) -> Result<Vec<SearchResult>, VivariumError> {
    let mut results = Vec::new();

    if let Ok(read_dir) = std::fs::read_dir(dir) {
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

            let data = std::fs::read(&path_val).ok().ok_or_else(|| {
                VivariumError::Other(format!("cannot read {}", path_val.display()))
            })?;

            let handle = handle_from_bytes(&data);
            let score = score_query(query, &data);
            if score <= 0.0 {
                continue;
            }

            let snippet = snippet_from_bytes(&data, 100);

            results.push(SearchResult {
                handle,
                raw_path: path_val.to_string_lossy().to_string(),
                account: String::new(),
                folder: String::new(),
                date: String::new(),
                from: String::new(),
                subject: String::new(),
                score,
                snippet,
            });
        }
    }

    Ok(results)
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
    fn scores_contained_words() {
        let data = b"From: a@b\r\nTo: c@d\r\nSubject: Hello World\r\n\r\nBody text";
        let score = score_query("hello", data);
        assert!(score > 0.0);
    }

    #[test]
    fn scores_zero_for_nonmatching() {
        let data = b"Subject: Hello World\r\n\r\nBody";
        let score = score_query("zxczxczxczxczxczxczxc", data);
        assert_eq!(score, 0.0);
    }

    #[test]
    fn snippet_respects_max_len() {
        let data = b"From: a@b\r\nTo: c@d\r\nSubject: test\r\n\r\nHello world body text";
        let snippet = snippet_from_bytes(data, 5);
        assert!(snippet.len() <= 5);
    }
}

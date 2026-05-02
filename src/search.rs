use std::cmp::min;
use std::path::Path;

use crate::error::VivariumError;
use crate::message::MessageEntry;
use crate::store::MailStore;

/// A search result with handle and citation metadata.
#[derive(Debug, Clone)]
pub struct SearchResult {
    pub handle: String,
    pub raw_path: String,
    pub account: String,
    pub folder: String,
    pub maildir_subdir: String,
    pub date: String,
    pub from: String,
    pub subject: String,
    pub score: f64,
    pub snippet: String,
}

/// Keyword search over the catalog and extracted text.
pub fn keyword_search(
    mail_root: &Path,
    account: &str,
    query: &str,
    limit: usize,
    offset: usize,
) -> Result<(Vec<SearchResult>, usize), VivariumError> {
    let query_lower = query.to_ascii_lowercase();
    let store = MailStore::new(mail_root);

    let mut all_results: Vec<SearchResult> = Vec::new();

    for folder in &["INBOX", "Archive", "Sent", "Drafts"] {
        let folder_results = search_folder(&store, account, folder, &query_lower)?;
        all_results.extend(folder_results);
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
        trim_snippet_line(body, max_len)
    } else if let Some(body_start) = text.find("\n\n") {
        let body = &text[body_start + 2..];
        trim_snippet_line(body, max_len)
    } else {
        String::new()
    }
}

fn trim_snippet_line(body: &str, max_len: usize) -> String {
    body.lines()
        .next()
        .unwrap_or("")
        .chars()
        .take(max_len)
        .collect()
}

/// Search result in JSON-friendly format.
pub fn to_json_result(result: &SearchResult) -> serde_json::Value {
    serde_json::json!({
        "handle": result.handle,
        "raw_path": result.raw_path,
        "account": result.account,
        "folder": result.folder,
        "maildir_subdir": result.maildir_subdir,
        "date": result.date,
        "from": result.from,
        "subject": result.subject,
        "score": result.score,
        "snippet": result.snippet,
        "citation": {
            "handle": result.handle,
            "account": result.account,
            "folder": result.folder,
            "maildir_subdir": result.maildir_subdir,
            "raw_path": result.raw_path,
            "source_type": "rfc5322",
        },
    })
}

pub fn print_results(
    query: &str,
    limit: usize,
    offset: usize,
    results: Vec<SearchResult>,
    total: usize,
    as_json: bool,
) {
    if as_json {
        let output = serde_json::json!({
            "query": query,
            "total": total,
            "limit": limit,
            "offset": offset,
            "results": results.into_iter()
                .map(|r| to_json_result(&r))
                .collect::<Vec<_>>(),
        });
        println!(
            "{}",
            serde_json::to_string_pretty(&output).unwrap_or_else(|_| output.to_string())
        );
        return;
    }

    println!("search: {} results for '{}'", total, query);
    for result in &results {
        println!(
            "  {}  {:<16}  {}  {}",
            result.handle, result.date, result.from, result.subject
        );
        if !result.snippet.is_empty() {
            println!("    snippet: {}", result.snippet);
        }
    }
}

fn search_folder(
    store: &MailStore,
    account: &str,
    folder: &str,
    query: &str,
) -> Result<Vec<SearchResult>, VivariumError> {
    let mut results = Vec::new();

    for entry in store.list_messages(folder)? {
        let data = std::fs::read(&entry.path)?;
        let score = score_query(query, &data);
        if score <= 0.0 {
            continue;
        }

        results.push(search_result(account, folder, entry, &data, score));
    }

    Ok(results)
}

fn search_result(
    account: &str,
    folder: &str,
    entry: MessageEntry,
    data: &[u8],
    score: f64,
) -> SearchResult {
    SearchResult {
        handle: entry.message_id,
        raw_path: entry.path.to_string_lossy().to_string(),
        account: account.to_string(),
        folder: folder.to_string(),
        maildir_subdir: entry
            .path
            .parent()
            .and_then(|p| p.file_name())
            .and_then(|n| n.to_str())
            .unwrap_or("")
            .to_string(),
        date: entry.date.format("%Y-%m-%d %H:%M").to_string(),
        from: entry.from,
        subject: entry.subject,
        score,
        snippet: snippet_from_bytes(data, 100),
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

    #[test]
    fn keyword_search_matches_maildir_eml_files_with_metadata() {
        let tmp = tempfile::tempdir().unwrap();
        let store = MailStore::new(tmp.path());
        store
            .store_message(
                "inbox",
                "inbox-1",
                b"From: Agent <agent@example.com>\r\nTo: me@example.com\r\nDate: Sat, 2 May 2026 13:35:00 +0000\r\nSubject: Release notice\r\n\r\nRelease body",
            )
            .unwrap();

        let (results, total) = keyword_search(tmp.path(), "acct", "release", 10, 0).unwrap();

        assert_eq!(total, 1);
        assert_eq!(results[0].handle, "inbox-1");
        assert_eq!(results[0].account, "acct");
        assert_eq!(results[0].folder, "INBOX");
        assert_eq!(results[0].maildir_subdir, "new");
        assert_eq!(results[0].from, "Agent");
        assert_eq!(results[0].subject, "Release notice");
        assert!(results[0].raw_path.ends_with("inbox-1.eml"));
    }

    #[test]
    fn json_result_includes_citation() {
        let result = SearchResult {
            handle: "inbox-1".into(),
            raw_path: "/tmp/inbox-1.eml".into(),
            account: "acct".into(),
            folder: "INBOX".into(),
            maildir_subdir: "new".into(),
            date: "2026-05-02 12:00".into(),
            from: "Agent".into(),
            subject: "Subject".into(),
            score: 1.0,
            snippet: "body".into(),
        };

        let json = to_json_result(&result);

        assert_eq!(json["citation"]["handle"], "inbox-1");
        assert_eq!(json["citation"]["account"], "acct");
        assert_eq!(json["citation"]["source_type"], "rfc5322");
    }
}

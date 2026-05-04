use std::path::Path;

use crate::email_index::{EmailIndex, IndexedMessage};
use crate::error::VivariumError;
use crate::storage::Storage;

mod output;
mod semantic;
pub use output::{SearchOutput, print_search_output, to_json_result};
pub use semantic::semantic_or_hybrid_search;

/// A search result with handle and citation metadata.
#[derive(Debug, Clone)]
pub struct SearchResult {
    pub handle: String,
    pub message_id: String,
    pub account: String,
    pub content_id: String,
    pub local_role: String,
    pub date: String,
    pub from: String,
    pub subject: String,
    pub score: f64,
    pub lexical_score: Option<f64>,
    pub semantic_score: Option<f64>,
    pub chunk_id: Option<String>,
    pub snippet: String,
}

/// Keyword search over the catalog and extracted text.
pub fn keyword_search(
    mail_root: &Path,
    account: &str,
    query: &str,
    limit: usize,
    offset: usize,
    folder: Option<&str>,
) -> Result<(Vec<SearchResult>, usize), VivariumError> {
    indexed_lexical_results_page(mail_root, account, query, limit, offset, folder)
}

pub fn canonical_search_folder(folder: &str) -> Result<String, VivariumError> {
    match folder.to_ascii_lowercase().as_str() {
        "inbox" => Ok("inbox".into()),
        "archive" | "all" => Ok("archive".into()),
        "trash" | "deleted" => Ok("trash".into()),
        "sent" => Ok("sent".into()),
        "draft" | "drafts" => Ok("drafts".into()),
        _ => Err(VivariumError::Message(format!(
            "unsupported search folder '{folder}'; expected inbox, archive, trash, sent, or drafts"
        ))),
    }
}

pub(crate) fn filter_by_folder(
    results: Vec<SearchResult>,
    folder: Option<&str>,
) -> Vec<SearchResult> {
    let Some(folder) = folder else {
        return results;
    };
    results
        .into_iter()
        .filter(|result| result.local_role.eq_ignore_ascii_case(folder))
        .collect()
}

pub(crate) fn indexed_lexical_results(
    mail_root: &Path,
    account: &str,
    query: &str,
) -> Result<Vec<SearchResult>, VivariumError> {
    let (results, _) =
        indexed_lexical_results_page(mail_root, account, query, usize::MAX, 0, None)?;
    Ok(results)
}

pub(crate) fn indexed_lexical_results_page(
    mail_root: &Path,
    account: &str,
    query: &str,
    limit: usize,
    offset: usize,
    folder: Option<&str>,
) -> Result<(Vec<SearchResult>, usize), VivariumError> {
    let index = EmailIndex::open(mail_root)?;
    if index.count_messages(account)? == 0
        && Storage::open(mail_root)?.count_messages_for_account(account)? > 0
    {
        return Err(VivariumError::Message(format!(
            "email index is empty for account '{account}'; run `vivi index rebuild --account {account}` or `vivi sync --index --account {account}`"
        )));
    }
    let (matches, total) = index.search_messages(account, query, limit, offset, folder)?;
    let results = matches
        .into_iter()
        .map(|(message, score)| {
            let data = std::fs::read(&message.blob_path).unwrap_or_default();
            search_result(message, &data, score)
        })
        .collect();
    Ok((results, total))
}

/// Score a query against raw .eml bytes.
#[cfg(test)]
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

fn search_result(message: IndexedMessage, data: &[u8], score: f64) -> SearchResult {
    SearchResult {
        handle: message.handle,
        message_id: message.message_id,
        account: message.account,
        content_id: message.content_id,
        local_role: message.local_role,
        date: message.date,
        from: message.from_addr,
        subject: message.subject,
        score,
        lexical_score: Some(score),
        semantic_score: None,
        chunk_id: None,
        snippet: snippet_from_bytes(data, 100),
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::*;
    use crate::catalog::{Catalog, CatalogEntry};
    use crate::email_index;
    use crate::store::MailStore;

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
    fn keyword_search_matches_indexed_eml_files_with_metadata() {
        let tmp = tempfile::tempdir().unwrap();
        let store = MailStore::new(tmp.path());
        let path = store
            .store_message(
                "inbox",
                "inbox-1",
                b"From: Agent <agent@example.com>\r\nTo: me@example.com\r\nDate: Sat, 2 May 2026 13:35:00 +0000\r\nSubject: Release notice\r\n\r\nRelease body",
            )
            .unwrap();
        catalog(tmp.path(), "acct", &path, "INBOX");
        email_index::rebuild(tmp.path(), "acct").unwrap();

        let (results, total) = keyword_search(tmp.path(), "acct", "release", 10, 0, None).unwrap();

        assert_eq!(total, 1);
        assert_eq!(results[0].message_id, "inbox-1");
        assert_eq!(results[0].account, "acct");
        assert_eq!(results[0].local_role, "inbox");
        assert_eq!(results[0].from, "Agent <agent@example.com>");
        assert_eq!(results[0].subject, "Release notice");
        assert!(!results[0].content_id.is_empty());
    }

    #[test]
    fn keyword_search_ignores_unindexed_maildir_files() {
        let tmp = tempfile::tempdir().unwrap();
        let store = MailStore::new(tmp.path());
        store
            .store_message(
                "inbox",
                "inbox-1",
                b"Subject: Release notice\r\n\r\nRelease body",
            )
            .unwrap();

        let (results, total) = keyword_search(tmp.path(), "acct", "release", 10, 0, None).unwrap();

        assert_eq!(total, 0);
        assert!(results.is_empty());
    }

    #[test]
    fn keyword_search_errors_when_catalog_exists_but_index_is_empty() {
        let tmp = tempfile::tempdir().unwrap();
        let store = MailStore::new(tmp.path());
        let path = store
            .store_message(
                "inbox",
                "inbox-1",
                b"Subject: Release notice\r\n\r\nRelease body",
            )
            .unwrap();
        catalog(tmp.path(), "acct", &path, "INBOX");

        let err = keyword_search(tmp.path(), "acct", "release", 10, 0, None).unwrap_err();

        assert!(err.to_string().contains("email index is empty"));
    }

    #[test]
    fn indexed_lexical_results_use_indexed_blob_content() {
        let tmp = tempfile::tempdir().unwrap();
        let store = MailStore::new(tmp.path());
        let path = store
            .store_message(
                "inbox",
                "inbox-1",
                b"Subject: Release notice\r\n\r\nRelease body",
            )
            .unwrap();
        catalog(tmp.path(), "acct", &path, "INBOX");
        email_index::rebuild(tmp.path(), "acct").unwrap();
        std::fs::write(&path, b"Subject: Release notice\r\n\r\nchanged").unwrap();

        let results = indexed_lexical_results(tmp.path(), "acct", "release").unwrap();

        assert_eq!(results.len(), 1);
    }

    #[test]
    fn json_result_includes_citation() {
        let result = SearchResult {
            handle: "inbox-1".into(),
            message_id: "inbox-1".into(),
            account: "acct".into(),
            content_id: "content-1".into(),
            local_role: "inbox".into(),
            date: "2026-05-02 12:00".into(),
            from: "Agent".into(),
            subject: "Subject".into(),
            score: 1.0,
            lexical_score: Some(1.0),
            semantic_score: None,
            chunk_id: None,
            snippet: "body".into(),
        };

        let json = to_json_result(&result);

        assert_eq!(json["citation"]["handle"], "inbox-1");
        assert_eq!(json["citation"]["account"], "acct");
        assert_eq!(json["citation"]["source_type"], "rfc5322");
    }

    fn catalog(mail_root: &Path, account: &str, path: &Path, folder: &str) {
        let data = std::fs::read(path).unwrap();
        let handle = path.file_stem().unwrap().to_string_lossy().to_string();
        let mut catalog = Catalog::open(mail_root).unwrap();
        catalog
            .upsert(&CatalogEntry {
                handle,
                account: account.into(),
                content_id: crate::catalog::fingerprint(&data),
                blob_path: path.to_string_lossy().to_string(),
                local_role: storage_role(folder),
                read_state: false,
                starred: false,
                date: "2026-05-02 13:35".into(),
                from: "Agent".into(),
                to: "me@example.com".into(),
                cc: String::new(),
                bcc: String::new(),
                subject: "Release notice".into(),
                rfc_message_id: crate::message::message_id_from_bytes(&data).unwrap_or_default(),
                remote: None,
            })
            .unwrap();
    }

    fn storage_role(folder: &str) -> String {
        match folder.to_ascii_lowercase().as_str() {
            "inbox" => "inbox".into(),
            "archive" => "archive".into(),
            "trash" => "trash".into(),
            "sent" => "sent".into(),
            "draft" | "drafts" => "drafts".into(),
            other => other.into(),
        }
    }
}

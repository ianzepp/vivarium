use std::path::Path;

use crate::email_index::{EmailIndex, IndexedMessage};
use crate::error::VivariumError;
use crate::storage::Storage;

mod filters;
mod output;
mod semantic;
pub use filters::SearchFilters;
pub(crate) use filters::filter_results;
pub use output::{SearchOutput, print_search_output, to_json_result};
pub use semantic::{SemanticSearchOptions, semantic_or_hybrid_search};

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
///
/// # Errors
/// Returns an error if the database query fails or the index cannot be opened.
pub fn keyword_search(
    mail_root: &Path,
    account: &str,
    query: &str,
    limit: usize,
    offset: usize,
    filters: Option<SearchFilters<'_>>,
) -> Result<(Vec<SearchResult>, usize), VivariumError> {
    indexed_lexical_results_page(mail_root, account, query, limit, offset, filters)
}

/// # Errors
/// Returns an error if the folder name is unrecognized.
pub fn canonical_search_folder(folder: &str) -> Result<String, VivariumError> {
    match folder.to_ascii_lowercase().as_str() {
        "inbox" => Ok("inbox".into()),
        "archive" | "all" => Ok("archive".into()),
        "trash" | "deleted" => Ok("trash".into()),
        "sent" => Ok("sent".into()),
        "draft" | "drafts" => Ok("drafts".into()),
        "task" | "tasks" => Ok("tasks".into()),
        "need" | "needs" => Ok("needs".into()),
        "want" | "wants" => Ok("wants".into()),
        "done" => Ok("done".into()),
        _ => Err(VivariumError::Message(format!(
            "unsupported search folder '{folder}'; expected inbox, archive, trash, sent, drafts, tasks, needs, wants, or done"
        ))),
    }
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
    filters: Option<SearchFilters<'_>>,
) -> Result<(Vec<SearchResult>, usize), VivariumError> {
    let index = EmailIndex::open(mail_root)?;
    if index.count_messages(account)? == 0
        && Storage::open(mail_root)?.count_messages_for_account(account)? > 0
    {
        return Err(VivariumError::Message(format!(
            "email index is empty for account '{account}'; run `vivi index rebuild --account {account}` or `vivi sync --index --account {account}`"
        )));
    }
    let (matches, total) = index.search_messages(account, query, limit, offset, filters)?;
    let results = matches
        .into_iter()
        .map(|(message, score)| {
            let data = std::fs::read(&message.blob_path).unwrap_or_default();
            search_result(message, &data, score)
        })
        .collect();
    Ok((results, total))
}

#[cfg(test)]
#[path = "search_test.rs"]
mod tests;

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

use std::path::Path;

use crate::email_index::{EmailIndex, IndexedMessage};
use crate::embeddings::{self, SemanticMatch};
use crate::error::VivariumError;
use crate::search::SearchResult;

pub async fn semantic_or_hybrid_search(
    mail_root: &Path,
    account: &str,
    query: &str,
    limit: usize,
    offset: usize,
    semantic: bool,
    hybrid: bool,
) -> Result<(Vec<SearchResult>, usize), VivariumError> {
    let (semantic_results, semantic_total) =
        semantic_results(mail_root, account, query, limit, offset).await?;
    if semantic && !hybrid {
        return Ok((semantic_results, semantic_total));
    }
    let lexical_results = indexed_lexical_search(mail_root, account, query)?;
    Ok(merge_hybrid(
        lexical_results,
        semantic_results,
        limit,
        offset,
    ))
}

async fn semantic_results(
    mail_root: &Path,
    account: &str,
    query: &str,
    limit: usize,
    offset: usize,
) -> Result<(Vec<SearchResult>, usize), VivariumError> {
    let (matches, total) = embeddings::semantic_search(
        mail_root,
        account,
        query,
        limit,
        offset,
        embeddings::EmbeddingOptions::default(),
    )
    .await?;
    Ok((matches.into_iter().map(semantic_result).collect(), total))
}

fn semantic_result(result: SemanticMatch) -> SearchResult {
    SearchResult {
        handle: result.handle,
        raw_path: result.raw_path,
        account: result.account,
        folder: result.folder,
        maildir_subdir: result.maildir_subdir,
        date: result.date,
        from: result.from,
        subject: result.subject,
        score: result.score,
        lexical_score: None,
        semantic_score: Some(result.score),
        chunk_id: Some(result.chunk_id),
        snippet: result.snippet,
    }
}

fn merge_hybrid(
    mut lexical: Vec<SearchResult>,
    semantic: Vec<SearchResult>,
    limit: usize,
    offset: usize,
) -> (Vec<SearchResult>, usize) {
    for semantic_result in semantic {
        merge_semantic_result(&mut lexical, semantic_result);
    }
    lexical.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    let total = lexical.len();
    let results = lexical.into_iter().skip(offset).take(limit).collect();
    (results, total)
}

fn merge_semantic_result(results: &mut Vec<SearchResult>, semantic: SearchResult) {
    if let Some(existing) = results
        .iter_mut()
        .find(|result| result.handle == semantic.handle)
    {
        existing.semantic_score = semantic.semantic_score;
        existing.chunk_id = semantic.chunk_id;
        existing.score += semantic.score;
        if existing.snippet.is_empty() {
            existing.snippet = semantic.snippet;
        }
    } else {
        results.push(semantic);
    }
}

fn indexed_lexical_search(
    mail_root: &Path,
    account: &str,
    query: &str,
) -> Result<Vec<SearchResult>, VivariumError> {
    let query_lower = query.to_ascii_lowercase();
    let index = EmailIndex::open(mail_root)?;
    let mut results = index
        .list_messages(account)?
        .into_iter()
        .filter_map(|message| indexed_lexical_result(message, &query_lower))
        .collect::<Vec<_>>();
    results.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    Ok(results)
}

fn indexed_lexical_result(message: IndexedMessage, query: &str) -> Option<SearchResult> {
    let text = indexed_lexical_text(&message);
    let score = score_text(query, &text);
    if score <= 0.0 {
        return None;
    }
    Some(SearchResult {
        handle: message.handle,
        raw_path: message.raw_path,
        account: message.account,
        folder: message.folder,
        maildir_subdir: message.maildir_subdir,
        date: message.date,
        from: message.from_addr,
        subject: message.subject.clone(),
        score,
        lexical_score: Some(score),
        semantic_score: None,
        chunk_id: None,
        snippet: message.subject,
    })
}

fn indexed_lexical_text(message: &IndexedMessage) -> String {
    format!(
        "{} {} {} {}",
        message.subject,
        message.from_addr,
        message.to_addr,
        message.rfc_message_id.clone().unwrap_or_default()
    )
    .to_ascii_lowercase()
}

fn score_text(query: &str, text: &str) -> f64 {
    let words = query.split_whitespace().collect::<Vec<_>>();
    if words.is_empty() {
        return 0.0;
    }
    let score = words
        .iter()
        .filter(|word| text.contains(**word))
        .map(|word| if word.len() > 3 { 2.0 } else { 1.0 })
        .sum::<f64>();
    score / words.len() as f64
}

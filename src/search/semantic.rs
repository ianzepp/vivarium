use std::path::Path;

use crate::embeddings::{self, SemanticMatch};
use crate::error::VivariumError;
use crate::search::{SearchFilters, SearchResult};

#[derive(Debug, Clone, Copy)]
pub struct SemanticSearchOptions<'a> {
    pub limit: usize,
    pub offset: usize,
    pub semantic: bool,
    pub hybrid: bool,
    pub filters: Option<SearchFilters<'a>>,
}

pub async fn semantic_or_hybrid_search(
    mail_root: &Path,
    account: &str,
    query: &str,
    options: SemanticSearchOptions<'_>,
) -> Result<(Vec<SearchResult>, usize), VivariumError> {
    let (semantic_results, semantic_total) =
        semantic_results(mail_root, account, query, options.limit, options.offset).await?;
    if options.semantic && !options.hybrid {
        if options.filters.is_none() {
            return Ok((semantic_results, semantic_total));
        }
        let results = super::filter_results(semantic_results, options.filters);
        return Ok((results.clone(), results.len()));
    }
    let lexical_results = super::filter_results(
        super::indexed_lexical_results(mail_root, account, query)?,
        options.filters,
    );
    let semantic_results = super::filter_results(semantic_results, options.filters);
    Ok(merge_hybrid(
        lexical_results,
        semantic_results,
        options.limit,
        options.offset,
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
        message_id: result.message_id,
        account: result.account,
        content_id: result.content_id,
        local_role: result.local_role,
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
        .find(|result| result.message_id == semantic.message_id)
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

use std::collections::BTreeSet;
use std::fs;
use std::path::Path;

use crate::email_index::{EmailIndex, IndexedMessage};
use crate::error::VivariumError;
use chunk::EmailChunk;
use progress::EmbeddingProgress;

mod chunk;
#[cfg(test)]
mod filter_tests;
mod progress;
mod provider;
mod store;
#[cfg(test)]
mod tests;

use provider::EmbeddingProvider;
pub use provider::OllamaEmbeddingProvider;

pub const DEFAULT_PROVIDER: &str = "ollama";
pub const DEFAULT_MODEL: &str = "cassio-embedding";
pub const DEFAULT_ENDPOINT: &str = "http://127.0.0.1:11434/api/embed";

#[derive(Debug, Clone)]
pub struct EmbeddingOptions {
    pub provider: String,
    pub model: String,
    pub endpoint: String,
    pub rebuild: bool,
    pub limit: Option<usize>,
    pub catalog_handles: Option<BTreeSet<String>>,
}

impl Default for EmbeddingOptions {
    fn default() -> Self {
        Self {
            provider: DEFAULT_PROVIDER.to_string(),
            model: DEFAULT_MODEL.to_string(),
            endpoint: DEFAULT_ENDPOINT.to_string(),
            rebuild: false,
            limit: None,
            catalog_handles: None,
        }
    }
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct EmbeddingStats {
    pub scanned: usize,
    pub reused: usize,
    pub embedded: usize,
    pub stale: usize,
    pub errors: usize,
}

#[derive(Debug, Clone)]
pub struct SemanticMatch {
    pub handle: String,
    pub account: String,
    pub folder: String,
    pub maildir_subdir: String,
    pub raw_path: String,
    pub date: String,
    pub from: String,
    pub subject: String,
    pub chunk_id: String,
    pub score: f64,
    pub snippet: String,
}

pub async fn index_embeddings(
    mail_root: &Path,
    account: &str,
    options: EmbeddingOptions,
) -> Result<EmbeddingStats, VivariumError> {
    if options.provider != DEFAULT_PROVIDER {
        return Err(VivariumError::Config(format!(
            "unsupported embedding provider '{}'; only '{DEFAULT_PROVIDER}' is supported",
            options.provider
        )));
    }
    let provider = OllamaEmbeddingProvider::new(
        options.provider.clone(),
        options.model.clone(),
        options.endpoint.clone(),
    );
    index_embeddings_with_provider(mail_root, account, options, &provider).await
}

async fn index_embeddings_with_provider<P: provider::EmbeddingProvider + Sync>(
    mail_root: &Path,
    account: &str,
    options: EmbeddingOptions,
    provider: &P,
) -> Result<EmbeddingStats, VivariumError> {
    let index = EmailIndex::open(mail_root)?;
    let mut store = store::EmbeddingStore::open(mail_root, provider.provider(), provider.model())?;
    let messages = filtered_messages(&index, account, options.catalog_handles.as_ref())?;
    let total_messages = options
        .limit
        .map(|limit| usize::min(limit, messages.len()))
        .unwrap_or(messages.len());
    let mut stats = EmbeddingStats::default();
    let mut retained_chunks = Vec::new();
    let mut staged_embeddings = Vec::new();
    let mut progress = EmbeddingProgress::start(
        account,
        provider.provider(),
        provider.model(),
        &options,
        total_messages,
    );

    for message in limited(messages, options.limit) {
        stats.scanned += 1;
        let indexed =
            index_message(&mut store, provider, &message, options.rebuild, &mut stats).await?;
        retained_chunks.extend(indexed.retained);
        staged_embeddings.extend(indexed.embeddings);
        progress.maybe_log(&stats, staged_embeddings.len());
    }
    if options.rebuild && options.limit.is_none() && stats.errors == 0 && stats.stale == 0 {
        progress.log_replacing(retained_chunks.len(), staged_embeddings.len());
        store.replace_account_embeddings(
            account,
            &retained_chunks,
            &staged_embeddings,
            provider.provider(),
            provider.model(),
        )?;
    } else if options.rebuild && !staged_embeddings.is_empty() {
        progress.log_partial_store(staged_embeddings.len());
        store.store_embedding_batch(&staged_embeddings, provider.provider(), provider.model())?;
    }
    progress.finish(&stats);
    Ok(stats)
}

fn filtered_messages(
    index: &EmailIndex,
    account: &str,
    catalog_handles: Option<&BTreeSet<String>>,
) -> Result<Vec<IndexedMessage>, VivariumError> {
    let Some(catalog_handles) = catalog_handles else {
        return index.list_messages(account);
    };
    if catalog_handles.is_empty() {
        return Ok(Vec::new());
    }
    Ok(index
        .list_messages(account)?
        .into_iter()
        .filter(|message| catalog_handles.contains(&message.catalog_handle))
        .collect())
}

struct IndexedEmbeddings {
    retained: Vec<String>,
    embeddings: Vec<(EmailChunk, Vec<f32>)>,
}

async fn index_message<P: provider::EmbeddingProvider + Sync>(
    store: &mut store::EmbeddingStore,
    provider: &P,
    message: &IndexedMessage,
    rebuild: bool,
    stats: &mut EmbeddingStats,
) -> Result<IndexedEmbeddings, VivariumError> {
    let Some(chunks) = message_chunks(message, stats)? else {
        return Ok(empty_indexed_embeddings());
    };
    let retained = chunks
        .iter()
        .map(|chunk| chunk.chunk_id.clone())
        .collect::<Vec<_>>();
    let chunk_count = chunks.len();
    let pending = if rebuild {
        chunks
    } else {
        store.pending_chunks(&chunks)?
    };
    stats.reused += chunk_count.saturating_sub(pending.len());
    if pending.is_empty() {
        return Ok(IndexedEmbeddings {
            retained,
            embeddings: Vec::new(),
        });
    }
    let texts = pending
        .iter()
        .map(|chunk| chunk.text.clone())
        .collect::<Vec<_>>();
    let vectors = provider.embed(&texts).await?;
    if vectors.len() != pending.len() {
        return Err(VivariumError::Other(format!(
            "embedding provider returned {} vectors for {} chunks",
            vectors.len(),
            pending.len()
        )));
    }
    stats.embedded += pending.len();
    if rebuild {
        return Ok(IndexedEmbeddings {
            retained,
            embeddings: pending.into_iter().zip(vectors).collect(),
        });
    }
    store.store_embeddings(&pending, provider.provider(), provider.model(), vectors)?;
    Ok(IndexedEmbeddings {
        retained,
        embeddings: Vec::new(),
    })
}

fn message_chunks(
    message: &IndexedMessage,
    stats: &mut EmbeddingStats,
) -> Result<Option<Vec<EmailChunk>>, VivariumError> {
    let data = match fs::read(&message.raw_path) {
        Ok(data) => data,
        Err(_) => {
            stats.errors += 1;
            return Ok(None);
        }
    };
    if crate::catalog::fingerprint(&data) != message.fingerprint {
        stats.stale += 1;
        return Ok(None);
    }
    match chunk::chunks_for_message(message, &data) {
        Ok(chunks) => Ok(Some(chunks)),
        Err(_) => {
            stats.errors += 1;
            Ok(None)
        }
    }
}

fn empty_indexed_embeddings() -> IndexedEmbeddings {
    IndexedEmbeddings {
        retained: Vec::new(),
        embeddings: Vec::new(),
    }
}

fn limited<T>(items: Vec<T>, limit: Option<usize>) -> Vec<T> {
    if let Some(limit) = limit {
        items.into_iter().take(limit).collect()
    } else {
        items
    }
}

pub async fn semantic_search(
    mail_root: &Path,
    account: &str,
    query: &str,
    limit: usize,
    offset: usize,
    options: EmbeddingOptions,
) -> Result<(Vec<SemanticMatch>, usize), VivariumError> {
    if options.provider != DEFAULT_PROVIDER {
        return Err(VivariumError::Config(format!(
            "unsupported embedding provider '{}'; only '{DEFAULT_PROVIDER}' is supported",
            options.provider
        )));
    }
    let provider = OllamaEmbeddingProvider::new(options.provider, options.model, options.endpoint);
    let query_vectors = provider.embed(&[query.to_string()]).await?;
    let query_vector = query_vectors.first().ok_or_else(|| {
        VivariumError::Other("embedding provider returned no query vector".into())
    })?;
    let index = EmailIndex::open(mail_root)?;
    let store = store::EmbeddingStore::open(mail_root, provider.provider(), provider.model())?;
    let scored = score_embeddings(&store.embeddings(account)?, query_vector);
    let total = scored.len();
    let matches = hydrate_matches(index, account, scored, limit, offset)?;
    Ok((matches, total))
}

fn score_embeddings(
    embeddings: &[store::StoredEmbedding],
    query_vector: &[f32],
) -> Vec<(store::StoredEmbedding, f64)> {
    let mut scored = embeddings
        .iter()
        .cloned()
        .filter_map(|embedding| {
            cosine_similarity(query_vector, &embedding.vector).map(|score| (embedding, score))
        })
        .collect::<Vec<_>>();
    scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    scored
}

fn hydrate_matches(
    index: EmailIndex,
    account: &str,
    scored: Vec<(store::StoredEmbedding, f64)>,
    limit: usize,
    offset: usize,
) -> Result<Vec<SemanticMatch>, VivariumError> {
    let mut matches = Vec::new();
    for (embedding, score) in scored.into_iter().skip(offset).take(limit) {
        if let Some(message) = index.message(account, &embedding.handle)?
            && let Some(snippet) = snippet_for_embedding(&message, &embedding)?
        {
            matches.push(match_from_message(message, embedding, score, snippet));
        }
    }
    Ok(matches)
}

fn match_from_message(
    message: IndexedMessage,
    embedding: store::StoredEmbedding,
    score: f64,
    snippet: String,
) -> SemanticMatch {
    SemanticMatch {
        handle: message.handle,
        account: message.account,
        folder: message.folder,
        maildir_subdir: message.maildir_subdir,
        raw_path: message.raw_path,
        date: message.date,
        from: message.from_addr,
        subject: message.subject,
        chunk_id: embedding.chunk_id,
        score,
        snippet,
    }
}

fn snippet_for_embedding(
    message: &IndexedMessage,
    embedding: &store::StoredEmbedding,
) -> Result<Option<String>, VivariumError> {
    if message.fingerprint != embedding.fingerprint || message.account != embedding.account {
        return Ok(None);
    }
    let data = fs::read(&message.raw_path)?;
    let chunks = chunk::chunks_for_message(message, &data)?;
    let snippet = chunks
        .into_iter()
        .find(|chunk| {
            chunk.chunk_id == embedding.chunk_id
                && chunk.chunk_ordinal == embedding.chunk_ordinal
                && chunk.text_hash == embedding.text_hash
        })
        .map(|chunk| trim_snippet(&chunk.text, 160));
    Ok(snippet)
}

fn trim_snippet(text: &str, max_chars: usize) -> String {
    text.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .chars()
        .take(max_chars)
        .collect()
}

fn cosine_similarity(a: &[f32], b: &[f32]) -> Option<f64> {
    if a.len() != b.len() || a.is_empty() {
        return None;
    }
    let dot = a
        .iter()
        .zip(b)
        .map(|(x, y)| f64::from(*x) * f64::from(*y))
        .sum::<f64>();
    let a_norm = a.iter().map(|v| f64::from(*v).powi(2)).sum::<f64>().sqrt();
    let b_norm = b.iter().map(|v| f64::from(*v).powi(2)).sum::<f64>().sqrt();
    if a_norm == 0.0 || b_norm == 0.0 {
        None
    } else {
        Some(dot / (a_norm * b_norm))
    }
}

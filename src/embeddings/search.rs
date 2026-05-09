use std::fs;
use std::path::Path;

use crate::email_index::{EmailIndex, IndexedMessage};
use crate::error::VivariumError;

use super::provider::EmbeddingProvider;
use super::{
    EmbeddingOptions, OllamaEmbeddingProvider, SUPPORTED_PROVIDER, SemanticMatch, chunk, store,
};

pub async fn semantic_search(
    mail_root: &Path,
    account: &str,
    query: &str,
    limit: usize,
    offset: usize,
    options: EmbeddingOptions,
) -> Result<(Vec<SemanticMatch>, usize), VivariumError> {
    if options.provider != SUPPORTED_PROVIDER {
        return Err(VivariumError::Config(format!(
            "unsupported embedding provider '{}'; only '{SUPPORTED_PROVIDER}' is supported",
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
        if let Some(message) = index.message(account, &embedding.message_id)?
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
        message_id: message.message_id,
        account: message.account,
        content_id: message.content_id,
        local_role: message.local_role,
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
    if message.content_id != embedding.content_id || message.account != embedding.account {
        return Ok(None);
    }
    let data = fs::read(&message.blob_path)?;
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

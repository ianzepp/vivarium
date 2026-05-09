use std::collections::BTreeSet;
use std::fs;
use std::path::Path;

use crate::config::Config;
use crate::email_index::{EmailIndex, IndexedMessage};
use crate::error::VivariumError;
use chunk::EmailChunk;
use progress::EmbeddingProgress;

mod chunk;
#[cfg(test)]
mod filter_tests;
#[cfg(test)]
mod option_tests;
mod progress;
mod provider;
mod search;
mod store;
#[cfg(test)]
mod tests;

pub use provider::OllamaEmbeddingProvider;
pub use search::semantic_search;

pub const SUPPORTED_PROVIDER: &str = "ollama";

#[derive(Debug, Clone)]
pub struct EmbeddingOptions {
    pub provider: String,
    pub model: String,
    pub endpoint: String,
    pub rebuild: bool,
    pub limit: Option<usize>,
    pub catalog_handles: Option<BTreeSet<String>>,
}

impl EmbeddingOptions {
    pub fn from_config(config: &Config) -> Result<Self, VivariumError> {
        Self::from_values(config, None, None, None)
    }

    pub fn from_values(
        config: &Config,
        provider: Option<String>,
        model: Option<String>,
        endpoint: Option<String>,
    ) -> Result<Self, VivariumError> {
        let provider = required_embedding_setting(
            provider
                .as_deref()
                .or(config.defaults.embedding_provider.as_deref()),
            "embedding_provider",
        )?;
        let model = required_embedding_setting(
            model
                .as_deref()
                .or(config.defaults.embedding_model.as_deref()),
            "embedding_model",
        )?;
        let endpoint = required_embedding_setting(
            endpoint
                .as_deref()
                .or(config.defaults.embedding_endpoint.as_deref()),
            "embedding_endpoint",
        )?;
        Ok(Self {
            provider,
            model,
            endpoint,
            rebuild: false,
            limit: None,
            catalog_handles: None,
        })
    }
}

fn required_embedding_setting(value: Option<&str>, key: &str) -> Result<String, VivariumError> {
    value
        .filter(|value| !value.trim().is_empty())
        .map(|value| value.trim().to_string())
        .ok_or_else(|| {
            VivariumError::Config(format!(
                "semantic embeddings require defaults.{key} in config.toml or explicit embedding CLI flags"
            ))
        })
}

#[cfg(test)]
fn test_embedding_options() -> EmbeddingOptions {
    EmbeddingOptions {
        provider: SUPPORTED_PROVIDER.to_string(),
        model: "test-embedding".to_string(),
        endpoint: "http://127.0.0.1:0/api/embed".to_string(),
        rebuild: false,
        limit: None,
        catalog_handles: None,
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
    pub message_id: String,
    pub account: String,
    pub content_id: String,
    pub local_role: String,
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
    if options.provider != SUPPORTED_PROVIDER {
        return Err(VivariumError::Config(format!(
            "unsupported embedding provider '{}'; only '{SUPPORTED_PROVIDER}' is supported",
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
        .filter(|message| catalog_handles.contains(&message.message_id))
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
    let data = match fs::read(&message.blob_path) {
        Ok(data) => data,
        Err(_) => {
            stats.errors += 1;
            return Ok(None);
        }
    };
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

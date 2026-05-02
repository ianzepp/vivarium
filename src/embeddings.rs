use std::fs;
use std::path::Path;

use crate::email_index::{EmailIndex, IndexedMessage};
use crate::error::VivariumError;

mod chunk;
mod provider;
mod store;
#[cfg(test)]
mod tests;

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
}

impl Default for EmbeddingOptions {
    fn default() -> Self {
        Self {
            provider: DEFAULT_PROVIDER.to_string(),
            model: DEFAULT_MODEL.to_string(),
            endpoint: DEFAULT_ENDPOINT.to_string(),
            rebuild: false,
            limit: None,
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
    if options.rebuild {
        store.clear_account(account)?;
    }
    let messages = index.list_messages(account)?;
    let mut stats = EmbeddingStats::default();

    for message in limited(messages, options.limit) {
        stats.scanned += 1;
        index_message(&mut store, provider, &message, &mut stats).await?;
    }
    Ok(stats)
}

async fn index_message<P: provider::EmbeddingProvider + Sync>(
    store: &mut store::EmbeddingStore,
    provider: &P,
    message: &IndexedMessage,
    stats: &mut EmbeddingStats,
) -> Result<(), VivariumError> {
    let data = match fs::read(&message.raw_path) {
        Ok(data) => data,
        Err(_) => {
            stats.errors += 1;
            return Ok(());
        }
    };
    if crate::catalog::fingerprint(&data) != message.fingerprint {
        stats.stale += 1;
        return Ok(());
    }
    let chunks = match chunk::chunks_for_message(message, &data) {
        Ok(chunks) => chunks,
        Err(_) => {
            stats.errors += 1;
            return Ok(());
        }
    };
    let pending = store.pending_chunks(&chunks)?;
    stats.reused += chunks.len().saturating_sub(pending.len());
    if pending.is_empty() {
        return Ok(());
    }
    let texts = pending
        .iter()
        .map(|chunk| chunk.text.clone())
        .collect::<Vec<_>>();
    let vectors = provider.embed(&texts).await?;
    store.store_embeddings(&pending, provider.provider(), provider.model(), vectors)?;
    stats.embedded += pending.len();
    Ok(())
}

fn limited<T>(items: Vec<T>, limit: Option<usize>) -> Vec<T> {
    if let Some(limit) = limit {
        items.into_iter().take(limit).collect()
    } else {
        items
    }
}

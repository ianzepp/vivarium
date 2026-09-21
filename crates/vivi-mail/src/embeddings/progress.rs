use std::time::{Duration, Instant};

use super::{EmbeddingOptions, EmbeddingStats};

pub(super) struct EmbeddingProgress<'a> {
    account: &'a str,
    provider: &'a str,
    model: &'a str,
    total_messages: usize,
    started: Instant,
    last_progress: Instant,
}

impl<'a> EmbeddingProgress<'a> {
    pub(super) fn start(
        account: &'a str,
        provider: &'a str,
        model: &'a str,
        options: &EmbeddingOptions,
        total_messages: usize,
    ) -> Self {
        tracing::info!(
            account,
            provider,
            model,
            rebuild = options.rebuild,
            limit = options.limit,
            total_messages,
            "embedding index started"
        );
        let now = Instant::now();
        Self {
            account,
            provider,
            model,
            total_messages,
            started: now,
            last_progress: now,
        }
    }

    pub(super) fn maybe_log(&mut self, stats: &EmbeddingStats, staged_embeddings: usize) {
        if !self.should_log(stats.scanned) {
            return;
        }
        tracing::info!(
            account = self.account,
            scanned = stats.scanned,
            total = self.total_messages,
            reused = stats.reused,
            embedded = stats.embedded,
            stale = stats.stale,
            errors = stats.errors,
            staged_embeddings,
            elapsed_secs = self.started.elapsed().as_secs(),
            "embedding progress"
        );
        self.last_progress = Instant::now();
    }

    pub(super) fn log_replacing(&self, retained_chunks: usize, embeddings: usize) {
        tracing::info!(
            account = self.account,
            retained_chunks,
            embeddings,
            "replacing account embeddings"
        );
    }

    pub(super) fn log_partial_store(&self, embeddings: usize) {
        tracing::info!(
            account = self.account,
            embeddings,
            "storing partial rebuilt embeddings"
        );
    }

    pub(super) fn finish(&self, stats: &EmbeddingStats) {
        tracing::info!(
            account = self.account,
            provider = self.provider,
            model = self.model,
            scanned = stats.scanned,
            reused = stats.reused,
            embedded = stats.embedded,
            stale = stats.stale,
            errors = stats.errors,
            elapsed_secs = self.started.elapsed().as_secs(),
            "embedding index finished"
        );
    }

    fn should_log(&self, scanned: usize) -> bool {
        scanned == self.total_messages
            || scanned == 1
            || self.last_progress.elapsed() >= Duration::from_secs(15)
    }
}

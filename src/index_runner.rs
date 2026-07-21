use std::path::Path;

use super::{Runtime, VivariumError};
use vivarium::cli::IndexCommand;
use vivarium::storage::Storage;

impl Runtime {
    pub(crate) async fn index(&self, command: IndexCommand) -> Result<(), VivariumError> {
        let acct = self.resolve_account(self.account.clone())?;
        let mail_root = acct.mail_path(&self.config);
        match command {
            IndexCommand::Rebuild => run_index_rebuild(&mail_root, &acct.name),
            IndexCommand::Status => run_index_status(&mail_root, &acct.name),
            IndexCommand::Pending => run_index_pending(&mail_root, &acct.name),
            IndexCommand::Embeddings {
                pending: _,
                rebuild,
                limit,
                provider,
                model,
                endpoint,
            } => {
                if !acct.allows_semantic_indexing() {
                    return Err(VivariumError::Config(format!(
                        "account '{}' uses storage_mode = \"{}\"; embeddings require storage_mode = \"semantic\"",
                        acct.name,
                        acct.resolved_storage_mode()
                    )));
                }
                let mut options = vivarium::embeddings::EmbeddingOptions::from_values(
                    &self.config,
                    provider.as_deref(),
                    model.as_deref(),
                    endpoint.as_deref(),
                )?;
                options.rebuild = rebuild;
                options.limit = limit;
                run_index_embeddings(&mail_root, &acct.name, options).await
            }
        }
    }
}

fn run_index_rebuild(mail_root: &Path, account: &str) -> Result<(), VivariumError> {
    let stats = vivarium::email_index::rebuild(mail_root, account)?;
    println!(
        "indexed {account}: scanned={} updated={} reused={} stale={} errors={}",
        stats.scanned, stats.updated, stats.reused, stats.stale, stats.errors
    );
    Ok(())
}

fn index_counts(mail_root: &Path, account: &str) -> Result<(usize, usize), VivariumError> {
    let catalog_count = Storage::open(mail_root)?.count_messages_for_account(account)?;
    let index = vivarium::email_index::EmailIndex::open(mail_root)?;
    let indexed_count = index.count_messages(account)?;
    Ok((catalog_count, indexed_count))
}

fn run_index_status(mail_root: &Path, account: &str) -> Result<(), VivariumError> {
    let (catalog_count, indexed_count) = index_counts(mail_root, account)?;
    println!(
        "index {account}: catalog={catalog_count} indexed={indexed_count} pending={}",
        catalog_count.saturating_sub(indexed_count)
    );
    Ok(())
}

fn run_index_pending(mail_root: &Path, account: &str) -> Result<(), VivariumError> {
    let (catalog_count, indexed_count) = index_counts(mail_root, account)?;
    println!(
        "pending {account}: {}",
        catalog_count.saturating_sub(indexed_count)
    );
    Ok(())
}

async fn run_index_embeddings(
    mail_root: &Path,
    account: &str,
    options: vivarium::embeddings::EmbeddingOptions,
) -> Result<(), VivariumError> {
    let stats = vivarium::embeddings::index_embeddings(mail_root, account, options).await?;
    println!(
        "embedded {account}: scanned={} reused={} embedded={} stale={} errors={}",
        stats.scanned, stats.reused, stats.embedded, stats.stale, stats.errors
    );
    Ok(())
}

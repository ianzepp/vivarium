use std::path::Path;

use super::{Runtime, VivariumError};
use vivarium::cli::IndexCommand;

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
                let options = vivarium::embeddings::EmbeddingOptions {
                    provider,
                    model,
                    endpoint,
                    rebuild,
                    limit,
                    catalog_handles: None,
                };
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
    let catalog = vivarium::catalog::Catalog::open(mail_root)?;
    let catalog_count = catalog.count_messages(account)?;
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

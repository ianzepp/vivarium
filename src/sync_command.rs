use super::{Runtime, VivariumError, print_sync_result};

pub(crate) struct SyncOptions {
    pub(crate) account: Option<String>,
    pub(crate) limit: Option<usize>,
    pub(crate) since: Option<String>,
    pub(crate) before: Option<String>,
    pub(crate) reset: bool,
    pub(crate) index: bool,
    pub(crate) embed: bool,
}

impl Runtime {
    pub(crate) async fn sync(&self, options: SyncOptions) -> Result<(), VivariumError> {
        validate_reset(
            options.reset,
            options.limit,
            options.since.as_deref(),
            options.before.as_deref(),
        )?;
        let window =
            vivarium::sync::SyncWindow::parse(options.since.as_deref(), options.before.as_deref())?;
        match self.selected_account_name(options.account) {
            Some(name) => {
                let acct = self.accounts.find_account(&name)?;
                if options.reset {
                    vivarium::sync::reset_account_cache(acct, &self.config)?;
                }
                let result = vivarium::sync::sync_account(
                    acct,
                    &self.config,
                    self.insecure,
                    options.limit,
                    window,
                )
                .await?;
                print_sync_result(&name, &result);
                self.run_post_sync_indexes(acct, options.index, options.embed)
                    .await?;
            }
            None => {
                for acct in &self.accounts.accounts {
                    if options.reset {
                        vivarium::sync::reset_account_cache(acct, &self.config)?;
                    }
                    let result = vivarium::sync::sync_account(
                        acct,
                        &self.config,
                        self.insecure,
                        options.limit,
                        window,
                    )
                    .await?;
                    print_sync_result(&acct.name, &result);
                    self.run_post_sync_indexes(acct, options.index, options.embed)
                        .await?;
                }
            }
        }
        Ok(())
    }

    async fn run_post_sync_indexes(
        &self,
        acct: &vivarium::config::Account,
        index: bool,
        embed: bool,
    ) -> Result<(), VivariumError> {
        if !index && !embed {
            return Ok(());
        }
        let mail_root = acct.mail_path(&self.config);
        let stats = vivarium::email_index::rebuild(&mail_root, &acct.name)?;
        println!(
            "indexed {}: scanned={} updated={} reused={} stale={} errors={}",
            acct.name, stats.scanned, stats.updated, stats.reused, stats.stale, stats.errors
        );
        if embed {
            let stats = vivarium::embeddings::index_embeddings(
                &mail_root,
                &acct.name,
                vivarium::embeddings::EmbeddingOptions::default(),
            )
            .await?;
            println!(
                "embedded {}: scanned={} reused={} embedded={} stale={} errors={}",
                acct.name, stats.scanned, stats.reused, stats.embedded, stats.stale, stats.errors
            );
        }
        Ok(())
    }
}

fn validate_reset(
    reset: bool,
    limit: Option<usize>,
    since: Option<&str>,
    before: Option<&str>,
) -> Result<(), VivariumError> {
    if !reset {
        return Ok(());
    }
    if limit.is_some() || since.is_some() || before.is_some() {
        return Err(VivariumError::Config(
            "--reset cannot be combined with --limit, --since, or --before".into(),
        ));
    }
    Ok(())
}

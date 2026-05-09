use std::collections::BTreeSet;

use serde::Serialize;

use vivarium::cli::Command;

use super::{Runtime, VivariumError, print_sync_result};

pub(crate) struct SyncOptions {
    pub(crate) account: Option<String>,
    pub(crate) limit: Option<usize>,
    pub(crate) since: Option<String>,
    pub(crate) before: Option<String>,
    pub(crate) reset: bool,
    pub(crate) index: bool,
    pub(crate) embed: bool,
    pub(crate) json: bool,
    pub(crate) all: bool,
}

impl SyncOptions {
    pub(crate) fn from_command(command: Command) -> Self {
        let Command::Sync {
            account,
            limit,
            since,
            before,
            reset,
            index,
            embed,
            json,
            all,
        } = command
        else {
            unreachable!();
        };
        Self {
            account,
            limit,
            since,
            before,
            reset,
            index,
            embed,
            json,
            all,
        }
    }
}

#[derive(Debug, Serialize)]
struct SyncReport {
    account: String,
    sync: SyncCountReport,
    index: Option<IndexCountReport>,
    embeddings: Option<EmbeddingCountReport>,
}

#[derive(Debug, Serialize)]
struct SyncCountReport {
    new: usize,
    archived: usize,
    cataloged: usize,
    extracted: usize,
    extraction_errors: usize,
    decryption_errors: usize,
}

#[derive(Debug, Serialize)]
struct IndexCountReport {
    scanned: usize,
    updated: usize,
    reused: usize,
    stale: usize,
    errors: usize,
}

#[derive(Debug, Serialize)]
struct EmbeddingCountReport {
    scanned: usize,
    reused: usize,
    embedded: usize,
    stale: usize,
    errors: usize,
}

#[derive(Debug, Default)]
struct PostSyncReports {
    index: Option<IndexCountReport>,
    embeddings: Option<EmbeddingCountReport>,
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
        let mut reports = Vec::new();
        match self.selected_account_name(options.account.clone()) {
            Some(name) => {
                let acct = self.accounts.find_account(&name)?;
                reports.push(self.sync_one_account(acct, &options, window).await?);
            }
            None => {
                for acct in &self.accounts.accounts {
                    reports.push(self.sync_one_account(acct, &options, window).await?);
                }
            }
        }
        if options.json {
            print_json_reports(&reports);
        }
        Ok(())
    }

    async fn sync_one_account(
        &self,
        acct: &vivarium::config::Account,
        options: &SyncOptions,
        window: vivarium::sync::SyncWindow,
    ) -> Result<SyncReport, VivariumError> {
        if options.reset {
            vivarium::sync::reset_account_cache(acct, &self.config)?;
        }
        let result = vivarium::sync::sync_account(
            acct,
            &self.config,
            self.insecure,
            options.limit,
            window,
            options.all,
        )
        .await?;
        if !options.json {
            print_sync_result(&acct.name, &result);
        }
        let post_sync = self
            .run_post_sync_indexes(acct, &result, options.index, options.embed, options.json)
            .await?;
        Ok(sync_report(acct.name.clone(), &result, post_sync))
    }

    async fn run_post_sync_indexes(
        &self,
        acct: &vivarium::config::Account,
        result: &vivarium::sync::SyncResult,
        index: bool,
        embed: bool,
        as_json: bool,
    ) -> Result<PostSyncReports, VivariumError> {
        if !index && !embed {
            return Ok(PostSyncReports::default());
        }
        let mail_root = acct.mail_path(&self.config);
        let index_report = Some(rebuild_index_report(&mail_root, &acct.name, as_json)?);
        let embeddings_report = if embed {
            Some(
                self.embedding_report(acct, result, &mail_root, as_json)
                    .await?,
            )
        } else {
            None
        };
        Ok(PostSyncReports {
            index: index_report,
            embeddings: embeddings_report,
        })
    }

    async fn embedding_report(
        &self,
        acct: &vivarium::config::Account,
        result: &vivarium::sync::SyncResult,
        mail_root: &std::path::Path,
        as_json: bool,
    ) -> Result<EmbeddingCountReport, VivariumError> {
        if !acct.allows_semantic_indexing() {
            return Err(VivariumError::Config(format!(
                "account '{}' uses storage_mode = \"{}\"; --embed requires storage_mode = \"semantic\"",
                acct.name,
                acct.resolved_storage_mode()
            )));
        }
        let catalog_handles = result
            .cataloged_entries
            .iter()
            .map(|entry| entry.handle.clone())
            .collect::<BTreeSet<_>>();
        let mut options = vivarium::embeddings::EmbeddingOptions::from_config(&self.config)?;
        options.catalog_handles = Some(catalog_handles);
        let stats = vivarium::embeddings::index_embeddings(mail_root, &acct.name, options).await?;
        if !as_json {
            print_embedding_stats(&acct.name, &stats);
        }
        Ok(EmbeddingCountReport {
            scanned: stats.scanned,
            reused: stats.reused,
            embedded: stats.embedded,
            stale: stats.stale,
            errors: stats.errors,
        })
    }
}

fn rebuild_index_report(
    mail_root: &std::path::Path,
    account: &str,
    as_json: bool,
) -> Result<IndexCountReport, VivariumError> {
    let stats = vivarium::email_index::rebuild(mail_root, account)?;
    if !as_json {
        println!(
            "indexed {account}: scanned={} updated={} reused={} stale={} errors={}",
            stats.scanned, stats.updated, stats.reused, stats.stale, stats.errors
        );
    }
    Ok(IndexCountReport {
        scanned: stats.scanned,
        updated: stats.updated,
        reused: stats.reused,
        stale: stats.stale,
        errors: stats.errors,
    })
}

fn print_embedding_stats(account: &str, stats: &vivarium::embeddings::EmbeddingStats) {
    println!(
        "embedded {account}: scanned={} reused={} embedded={} stale={} errors={}",
        stats.scanned, stats.reused, stats.embedded, stats.stale, stats.errors
    );
}

fn sync_report(
    account: String,
    result: &vivarium::sync::SyncResult,
    post_sync: PostSyncReports,
) -> SyncReport {
    SyncReport {
        account,
        sync: SyncCountReport {
            new: result.new,
            archived: result.archived,
            cataloged: result.cataloged,
            extracted: result.extracted,
            extraction_errors: result.extraction_errors,
            decryption_errors: result.decryption_errors,
        },
        index: post_sync.index,
        embeddings: post_sync.embeddings,
    }
}

fn print_json_reports(reports: &[SyncReport]) {
    println!("{}", render_json_reports(reports));
}

fn render_json_reports(reports: &[SyncReport]) -> String {
    if let [report] = reports {
        serde_json::to_string_pretty(report).unwrap_or_else(|_| "{}".into())
    } else {
        serde_json::to_string_pretty(reports).unwrap_or_else(|_| "[]".into())
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

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    #[test]
    fn sync_json_renders_single_account_as_object() {
        let report = test_report("agent-proton");
        let json: Value = serde_json::from_str(&render_json_reports(&[report])).unwrap();

        assert_eq!(json["account"], "agent-proton");
        assert_eq!(json["sync"]["new"], 2);
        assert_eq!(json["sync"]["decryption_errors"], 0);
        assert_eq!(json["index"], Value::Null);
        assert_eq!(json["embeddings"], Value::Null);
    }

    #[test]
    fn sync_json_renders_multiple_accounts_as_array() {
        let json: Value = serde_json::from_str(&render_json_reports(&[
            test_report("first"),
            test_report("second"),
        ]))
        .unwrap();

        assert_eq!(json.as_array().unwrap().len(), 2);
        assert_eq!(json[0]["account"], "first");
        assert_eq!(json[1]["account"], "second");
    }

    #[test]
    fn sync_json_includes_post_processing_reports() {
        let mut report = test_report("semantic");
        report.index = Some(IndexCountReport {
            scanned: 3,
            updated: 2,
            reused: 1,
            stale: 0,
            errors: 0,
        });
        report.embeddings = Some(EmbeddingCountReport {
            scanned: 3,
            reused: 1,
            embedded: 2,
            stale: 0,
            errors: 0,
        });

        let json: Value = serde_json::from_str(&render_json_reports(&[report])).unwrap();

        assert_eq!(json["index"]["updated"], 2);
        assert_eq!(json["embeddings"]["embedded"], 2);
    }

    fn test_report(account: &str) -> SyncReport {
        SyncReport {
            account: account.into(),
            sync: SyncCountReport {
                new: 2,
                archived: 0,
                cataloged: 2,
                extracted: 2,
                extraction_errors: 0,
                decryption_errors: 0,
            },
            index: None,
            embeddings: None,
        }
    }
}

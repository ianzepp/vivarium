use std::time::Duration;

use serde::Serialize;
use vivarium::cli::Command;
use vivarium::config::Provider;
use vivarium::proton_events::{ProtonEventSyncOptions, ProtonEventSyncReport};

use super::{MailStore, Runtime, VivariumError};

pub(crate) struct SyncEventsOptions {
    account: Option<String>,
    bootstrap: bool,
    watch: bool,
    interval: String,
    json: bool,
}

impl SyncEventsOptions {
    pub(crate) fn from_command(command: Command) -> Self {
        let Command::SyncEvents {
            account,
            bootstrap,
            watch,
            interval,
            json,
        } = command
        else {
            unreachable!();
        };
        Self {
            account,
            bootstrap,
            watch,
            interval,
            json,
        }
    }
}

#[derive(Debug, Serialize)]
struct SyncEventsReport {
    account: String,
    previous_event_id: Option<String>,
    event_id: Option<String>,
    bootstrapped: bool,
    events: usize,
    created: usize,
    updated: usize,
    deleted: usize,
    full_refreshes: usize,
    synced: usize,
    decryption_errors: usize,
}

impl Runtime {
    pub(crate) async fn sync_events(
        &self,
        options: SyncEventsOptions,
    ) -> Result<(), VivariumError> {
        let interval = parse_interval(&options.interval)?;
        loop {
            let report = self.sync_events_once(&options).await?;
            if options.json {
                println!("{}", render_report(&report));
            } else {
                print_report(&report);
            }
            if !options.watch {
                return Ok(());
            }
            tokio::time::sleep(interval).await;
        }
    }

    async fn sync_events_once(
        &self,
        options: &SyncEventsOptions,
    ) -> Result<SyncEventsReport, VivariumError> {
        let acct = self.resolve_account(self.selected_account_name(options.account.clone()))?;
        if acct.provider != Provider::ProtonApi {
            return Err(VivariumError::Config(format!(
                "account '{}' uses provider = \"{}\"; sync-events requires provider = \"proton-api\"",
                acct.name, acct.provider
            )));
        }
        let store = MailStore::new(&acct.mail_path(&self.config));
        let report = vivarium::proton_events::sync_events(
            &acct,
            &store,
            ProtonEventSyncOptions {
                bootstrap: options.bootstrap,
            },
        )
        .await?;
        Ok(report.into())
    }
}

impl From<ProtonEventSyncReport> for SyncEventsReport {
    fn from(report: ProtonEventSyncReport) -> Self {
        Self {
            account: report.account,
            previous_event_id: report.previous_event_id,
            event_id: report.event_id,
            bootstrapped: report.bootstrapped,
            events: report.events,
            created: report.created,
            updated: report.updated,
            deleted: report.deleted,
            full_refreshes: report.full_refreshes,
            synced: report.synced,
            decryption_errors: report.decryption_errors,
        }
    }
}

fn parse_interval(value: &str) -> Result<Duration, VivariumError> {
    vivarium::duration::parse_duration(value).map_err(|err| match err {
        VivariumError::Config(message) if message.contains("greater than zero") => {
            VivariumError::Config("--interval must be greater than zero".into())
        }
        VivariumError::Config(_) => VivariumError::Config(format!(
            "invalid --interval '{value}'; use values like 30s, 5m, or 1h"
        )),
        other => other,
    })
}

fn print_report(report: &SyncEventsReport) {
    println!(
        "sync-events {}: events={} created={} updated={} deleted={} synced={} refreshes={} cursor={}",
        report.account,
        report.events,
        report.created,
        report.updated,
        report.deleted,
        report.synced,
        report.full_refreshes,
        report.event_id.as_deref().unwrap_or("none")
    );
}

fn render_report(report: &SyncEventsReport) -> String {
    serde_json::to_string_pretty(report).unwrap_or_else(|_| "{}".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_interval_units() {
        assert_eq!(parse_interval("30").unwrap(), Duration::from_secs(30));
        assert_eq!(parse_interval("30s").unwrap(), Duration::from_secs(30));
        assert_eq!(parse_interval("5m").unwrap(), Duration::from_mins(5));
        assert_eq!(parse_interval("1h").unwrap(), Duration::from_hours(1));
    }

    #[test]
    fn rejects_invalid_interval() {
        assert!(parse_interval("0s").is_err());
        assert!(parse_interval("soon").is_err());
        assert!(parse_interval("5ms").is_err());
    }
}

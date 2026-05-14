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
    let value = value.trim();
    let Some((number, unit)) = split_interval(value) else {
        return Err(VivariumError::Config(format!(
            "invalid --interval '{value}'; use values like 30s, 5m, or 1h"
        )));
    };
    let amount = number.parse::<u64>().map_err(|_| {
        VivariumError::Config(format!(
            "invalid --interval '{value}'; use values like 30s, 5m, or 1h"
        ))
    })?;
    if amount == 0 {
        return Err(VivariumError::Config(
            "--interval must be greater than zero".into(),
        ));
    }
    let seconds = match unit {
        "" | "s" => amount,
        "m" => amount.saturating_mul(60),
        "h" => amount.saturating_mul(60 * 60),
        _ => {
            return Err(VivariumError::Config(format!(
                "invalid --interval unit '{unit}'; use s, m, or h"
            )));
        }
    };
    Ok(Duration::from_secs(seconds))
}

fn split_interval(value: &str) -> Option<(&str, &str)> {
    let first_unit = value
        .char_indices()
        .find_map(|(index, ch)| (!ch.is_ascii_digit()).then_some(index))
        .unwrap_or(value.len());
    if first_unit == 0 {
        return None;
    }
    Some((&value[..first_unit], &value[first_unit..]))
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
        assert_eq!(parse_interval("5m").unwrap(), Duration::from_secs(300));
        assert_eq!(parse_interval("1h").unwrap(), Duration::from_secs(3600));
    }

    #[test]
    fn rejects_invalid_interval() {
        assert!(parse_interval("0s").is_err());
        assert!(parse_interval("soon").is_err());
        assert!(parse_interval("5ms").is_err());
    }
}

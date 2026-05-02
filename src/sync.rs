use chrono::{DateTime, Local, Months, NaiveDate, Utc};

use crate::config::{Account, Config};
use crate::error::VivariumError;
use crate::store::MailStore;

#[derive(Debug, Default)]
pub struct SyncResult {
    pub new: usize,
    pub archived: usize,
    pub cataloged: usize,
    pub extracted: usize,
    pub extraction_errors: usize,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct SyncWindow {
    pub since: Option<NaiveDate>,
    pub before: Option<NaiveDate>,
}

impl SyncWindow {
    pub fn parse(since: Option<&str>, before: Option<&str>) -> Result<Self, VivariumError> {
        Ok(Self {
            since: since.map(parse_since).transpose()?,
            before: before.map(parse_absolute_date).transpose()?,
        })
    }

    pub fn is_empty(&self) -> bool {
        self.since.is_none() && self.before.is_none()
    }

    pub fn contains_datetime(&self, date: DateTime<Utc>) -> bool {
        let date = date.date_naive();
        self.since.is_none_or(|since| date >= since)
            && self.before.is_none_or(|before| date < before)
    }
}

pub async fn sync_account(
    account: &Account,
    config: &Config,
    insecure: bool,
    limit: Option<usize>,
    window: SyncWindow,
) -> Result<SyncResult, VivariumError> {
    let store = MailStore::new(&account.mail_path(config));
    store.ensure_folders()?;

    let mut result = if limit == Some(0) {
        SyncResult::default()
    } else {
        let reject_invalid_certs = account.reject_invalid_certs(config) && !insecure;
        crate::imap::sync_messages(account, &store, reject_invalid_certs, limit, window).await?
    };
    let catalog_update =
        crate::catalog::update_maildir(&account.mail_path(config), &account.name, &store)?;
    let (extracted, extraction_errors) =
        crate::extract::extract_catalog_entries(&catalog_update.entries)?;
    result.cataloged = catalog_update.cataloged;
    result.extracted = extracted;
    result.extraction_errors = extraction_errors;

    tracing::info!(
        account = account.name,
        new = result.new,
        cataloged = result.cataloged,
        extracted = result.extracted,
        extraction_errors = result.extraction_errors,
        "sync complete"
    );
    Ok(result)
}

fn parse_since(value: &str) -> Result<NaiveDate, VivariumError> {
    parse_absolute_date(value).or_else(|_| parse_relative_since(value))
}

fn parse_absolute_date(value: &str) -> Result<NaiveDate, VivariumError> {
    NaiveDate::parse_from_str(value, "%Y-%m-%d")
        .map_err(|_| VivariumError::Config(format!("invalid date '{value}', expected YYYY-MM-DD")))
}

fn parse_relative_since(value: &str) -> Result<NaiveDate, VivariumError> {
    let today = Local::now().date_naive();
    if let Some(count) = parse_suffix(value, "mo")? {
        return today.checked_sub_months(Months::new(count)).ok_or_else(|| {
            VivariumError::Config(format!("relative date is out of range: {value}"))
        });
    }
    if let Some(count) = parse_suffix(value, "y")? {
        return today
            .checked_sub_months(Months::new(count.saturating_mul(12)))
            .ok_or_else(|| {
                VivariumError::Config(format!("relative date is out of range: {value}"))
            });
    }
    if let Some(count) = parse_suffix(value, "w")? {
        return today
            .checked_sub_signed(chrono::Duration::weeks(i64::from(count)))
            .ok_or_else(|| {
                VivariumError::Config(format!("relative date is out of range: {value}"))
            });
    }
    if let Some(count) = parse_suffix(value, "d")? {
        return today
            .checked_sub_signed(chrono::Duration::days(i64::from(count)))
            .ok_or_else(|| {
                VivariumError::Config(format!("relative date is out of range: {value}"))
            });
    }
    Err(VivariumError::Config(format!(
        "invalid relative date '{value}', expected Nd, Nw, Nmo, Ny, or YYYY-MM-DD"
    )))
}

fn parse_suffix(value: &str, suffix: &str) -> Result<Option<u32>, VivariumError> {
    let Some(number) = value.strip_suffix(suffix) else {
        return Ok(None);
    };
    if number.is_empty() {
        return Err(VivariumError::Config(format!(
            "missing number in relative date '{value}'"
        )));
    }
    number
        .parse::<u32>()
        .map(Some)
        .map_err(|_| VivariumError::Config(format!("invalid number in relative date '{value}'")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    #[test]
    fn sync_window_parses_absolute_dates() {
        let window = SyncWindow::parse(Some("2026-02-01"), Some("2026-05-01")).unwrap();
        assert_eq!(
            window.since,
            Some(NaiveDate::from_ymd_opt(2026, 2, 1).unwrap())
        );
        assert_eq!(
            window.before,
            Some(NaiveDate::from_ymd_opt(2026, 5, 1).unwrap())
        );
    }

    #[test]
    fn sync_window_parses_relative_since() {
        let today = Local::now().date_naive();
        let window = SyncWindow::parse(Some("7d"), None).unwrap();
        assert_eq!(
            window.since,
            today.checked_sub_signed(chrono::Duration::days(7))
        );
    }

    #[test]
    fn sync_window_rejects_invalid_dates() {
        let err = SyncWindow::parse(Some("three months"), None).unwrap_err();
        assert!(err.to_string().contains("invalid relative date"));
    }

    #[test]
    fn sync_window_matches_datetimes() {
        let window = SyncWindow::parse(Some("2026-02-01"), Some("2026-03-01")).unwrap();
        let inside = Utc.with_ymd_and_hms(2026, 2, 12, 12, 0, 0).unwrap();
        let before = Utc.with_ymd_and_hms(2026, 1, 31, 12, 0, 0).unwrap();
        let after = Utc.with_ymd_and_hms(2026, 3, 1, 12, 0, 0).unwrap();

        assert!(window.contains_datetime(inside));
        assert!(!window.contains_datetime(before));
        assert!(!window.contains_datetime(after));
    }
}

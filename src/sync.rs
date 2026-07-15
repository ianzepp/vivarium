use std::fs;

use chrono::{DateTime, Local, Months, NaiveDate, Utc};

use crate::catalog::{CatalogEntry, RemoteIdentityCandidate};
use crate::config::{Account, Config, Provider};
use crate::error::VivariumError;
use crate::store::MailStore;

#[derive(Debug, Default)]
pub struct SyncResult {
    pub new: usize,
    pub archived: usize,
    pub cataloged: usize,
    pub extracted: usize,
    pub extraction_errors: usize,
    pub decryption_errors: usize,
    pub remote_identities: Vec<RemoteIdentityCandidate>,
    pub cataloged_entries: Vec<CatalogEntry>,
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
    all: bool,
) -> Result<SyncResult, VivariumError> {
    let store = MailStore::new(&account.mail_path(config));
    store.ensure_folders()?;

    let mut result = if limit == Some(0) {
        SyncResult::default()
    } else if matches!(account.provider, Provider::ProtonApi) {
        crate::proton_sync::sync_messages(account, &store, limit, window).await?
    } else {
        let reject_invalid_certs = account.reject_invalid_certs(config) && !insecure;
        crate::imap::sync_messages(account, &store, reject_invalid_certs, limit, window, all)
            .await?
    };
    let (extracted, extraction_errors) =
        crate::extract::extract_catalog_entries(&result.cataloged_entries)?;
    result.cataloged = result.cataloged_entries.len();
    result.extracted = extracted;
    result.extraction_errors = extraction_errors;

    tracing::info!(
        account = account.name,
        new = result.new,
        cataloged = result.cataloged,
        extracted = result.extracted,
        extraction_errors = result.extraction_errors,
        decryption_errors = result.decryption_errors,
        "sync complete"
    );
    Ok(result)
}

/// Reset (delete) the local account cache directory.
///
/// # Containment invariant
///
/// Reset may delete only a canonical account-owned cache directory. The path
/// must be a proper child of the configured mail root (managed path) or an
/// explicitly confirmed custom path. Root, home, cwd, repository roots, and
/// symlink escapes are always rejected. Failed validation performs no deletion.
///
/// - `confirm_custom` must be `true` when the account uses a custom `mail_dir`.
///   The CLI exposes this as `--confirm-reset`; callers cannot bypass it.
pub fn reset_account_cache(
    account: &Account,
    config: &Config,
    confirm_custom: bool,
) -> Result<(), VivariumError> {
    let mail_path = account.mail_path(config);
    validate_reset_path(account, config, &mail_path, confirm_custom)?;
    if mail_path.exists() {
        validate_reset_symlink(&mail_path)?;
        let canonical = mail_path.canonicalize().map_err(|e| {
            VivariumError::Other(format!(
                "cannot resolve reset path {}: {e}",
                mail_path.display()
            ))
        })?;
        fs::remove_dir_all(&canonical)?;
    }
    tracing::warn!(
        account = account.name,
        path = %mail_path.display(),
        "reset local account cache"
    );
    Ok(())
}

/// Validate that a reset path is safe to delete.
fn validate_reset_path(
    account: &Account,
    config: &Config,
    path: &std::path::Path,
    confirm_custom: bool,
) -> Result<(), VivariumError> {
    reject_system_directory(path)?;

    let managed_root = config
        .defaults
        .mail_root
        .as_deref()
        .map(crate::config::expand_tilde)
        .unwrap_or_else(Config::default_mail_root);
    if path == &managed_root {
        return Err(VivariumError::Other(format!(
            "reset target {} is the mail root, not an account directory; refusing",
            path.display()
        )));
    }

    let is_custom = account.mail_dir.is_some();
    if is_custom {
        return validate_custom_reset(account, path, confirm_custom);
    }

    validate_managed_reset(path, &managed_root)
}

fn reject_system_directory(path: &std::path::Path) -> Result<(), VivariumError> {
    let home = dirs::home_dir().unwrap_or_default();
    let cwd = std::env::current_dir().unwrap_or_default();
    let lower = path.to_string_lossy().to_ascii_lowercase();
    for d in ["/", &*home.to_string_lossy(), &*cwd.to_string_lossy()] {
        if path == std::path::Path::new(d) || lower == d.to_ascii_lowercase() {
            return Err(VivariumError::Other(format!(
                "reset target {} is a system directory; refusing to delete",
                path.display()
            )));
        }
    }
    Ok(())
}

fn validate_custom_reset(
    account: &Account,
    _path: &std::path::Path,
    confirm_custom: bool,
) -> Result<(), VivariumError> {
    if !confirm_custom {
        return Err(VivariumError::Other(format!(
            "account '{}' uses a custom mail_dir; reset requires --confirm-reset to proceed",
            account.name
        )));
    }
    Ok(())
}

fn validate_managed_reset(
    path: &std::path::Path,
    managed_root: &std::path::Path,
) -> Result<(), VivariumError> {
    if !path.starts_with(managed_root) {
        return Err(VivariumError::Other(format!(
            "managed reset path {} is outside the mail root {}; refusing",
            path.display(),
            managed_root.display()
        )));
    }
    if let Some(parent) = managed_root.parent() {
        if path == parent {
            return Err(VivariumError::Other(format!(
                "reset target {} is an ancestor of the mail root; refusing",
                path.display()
            )));
        }
    }
    Ok(())
}

/// Reject symlink escapes after canonicalization.
fn validate_reset_symlink(canonical: &std::path::Path) -> Result<(), VivariumError> {
    let metadata = std::fs::symlink_metadata(canonical)?;
    if metadata.file_type().is_symlink() {
        return Err(VivariumError::Other(format!(
            "reset target {} is a symlink; refusing to follow",
            canonical.display()
        )));
    }
    Ok(())
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

    #[test]
    fn reset_account_cache_removes_account_mail_path() {
        let tmp = tempfile::tempdir().unwrap();
        let account = account_with_mail_dir(tmp.path().join("account"));
        let config = Config::default();
        let message_path = account.mail_path(&config).join("INBOX/new/message.eml");
        std::fs::create_dir_all(message_path.parent().unwrap()).unwrap();
        std::fs::write(&message_path, b"Subject: hi\r\n\r\n").unwrap();

        reset_account_cache(&account, &config, true).unwrap();

        assert!(!account.mail_path(&config).exists());
    }

    #[test]
    fn reset_rejects_root_path() {
        let mut account = account_with_mail_dir(std::path::PathBuf::from("/"));
        let config = Config::default();
        let err = reset_account_cache(&account, &config, true).unwrap_err();
        assert!(err.to_string().contains("system directory"));
    }

    #[test]
    fn reset_rejects_home_directory() {
        let home = dirs::home_dir().unwrap();
        let account = account_with_mail_dir(home);
        let config = Config::default();
        let err = reset_account_cache(&account, &config, true).unwrap_err();
        assert!(err.to_string().contains("system directory"));
    }

    #[test]
    fn reset_rejects_cwd() {
        let cwd = std::env::current_dir().unwrap();
        let account = account_with_mail_dir(cwd);
        let config = Config::default();
        let err = reset_account_cache(&account, &config, true).unwrap_err();
        assert!(err.to_string().contains("system directory"));
    }

    #[test]
    fn reset_rejects_custom_path_without_confirmation() {
        let tmp = tempfile::tempdir().unwrap();
        let account = account_with_mail_dir(tmp.path().join("custom"));
        let config = Config::default();
        let err = reset_account_cache(&account, &config, false).unwrap_err();
        assert!(err.to_string().contains("--confirm-reset"));
    }

    #[test]
    fn reset_allows_custom_path_with_confirmation() {
        let tmp = tempfile::tempdir().unwrap();
        let account = account_with_mail_dir(tmp.path().join("custom"));
        let config = Config::default();
        let cache = account.mail_path(&config).join("INBOX/new");
        std::fs::create_dir_all(&cache).unwrap();
        std::fs::write(cache.join("msg.eml"), b"data").unwrap();

        reset_account_cache(&account, &config, true).unwrap();

        assert!(!account.mail_path(&config).exists());
    }

    #[test]
    fn reset_managed_path_works_without_confirmation() {
        let tmp = tempfile::tempdir().unwrap();
        let config = Config {
            defaults: crate::config::types::Defaults {
                mail_root: Some(tmp.path().to_string_lossy().to_string()),
                ..Default::default()
            },
        };
        let account = account_managed("managed-acct");
        let cache = account.mail_path(&config).join("INBOX/new");
        std::fs::create_dir_all(&cache).unwrap();
        std::fs::write(cache.join("msg.eml"), b"data").unwrap();

        reset_account_cache(&account, &config, false).unwrap();

        assert!(!account.mail_path(&config).exists());
    }

    #[test]
    fn reset_rejects_mail_root_itself() {
        let tmp = tempfile::tempdir().unwrap();
        let config = Config {
            defaults: crate::config::types::Defaults {
                mail_root: Some(tmp.path().to_string_lossy().to_string()),
                ..Default::default()
            },
        };
        // Simulate an account whose mail_dir is the mail root itself.
        let account = account_with_mail_dir(tmp.path().to_path_buf());
        let err = reset_account_cache(&account, &config, true).unwrap_err();
        assert!(err.to_string().contains("mail root"));
    }

    #[test]
    fn reset_failed_validation_does_not_delete() {
        let tmp = tempfile::tempdir().unwrap();
        let target = tmp.path().join("data");
        std::fs::create_dir_all(&target).unwrap();
        std::fs::write(target.join("important.eml"), b"keep me").unwrap();

        let account = account_with_mail_dir(target.clone());
        let config = Config::default();

        // Rejected because confirm_custom is false.
        let err = reset_account_cache(&account, &config, false).unwrap_err();
        assert!(err.to_string().contains("--confirm-reset"));

        // Data must still be there.
        assert!(target.join("important.eml").exists());
    }

    #[test]
    fn reset_rejects_symlink_escape() {
        let tmp = tempfile::tempdir().unwrap();
        let real = tmp.path().join("real");
        let link = tmp.path().join("link");
        std::fs::create_dir_all(&real).unwrap();
        std::fs::write(real.join("data.eml"), b"data").unwrap();
        std::os::unix::fs::symlink(&real, &link).unwrap();

        let account = account_with_mail_dir(link);
        let config = Config::default();

        let err = reset_account_cache(&account, &config, true).unwrap_err();
        assert!(err.to_string().contains("symlink"));
        // Target data must be intact.
        assert!(real.join("data.eml").exists());
    }

    fn account_managed(name: &str) -> Account {
        let mut account = account_with_mail_dir(std::path::PathBuf::from("/tmp"));
        account.name = name.into();
        account.mail_dir = None;
        account
    }

    fn account_with_mail_dir(mail_dir: std::path::PathBuf) -> Account {
        Account {
            name: "test".into(),
            email: "test@example.com".into(),
            imap_host: "localhost".into(),
            imap_port: Some(1143),
            imap_security: Some(crate::config::Security::Starttls),
            smtp_host: "localhost".into(),
            smtp_port: Some(1025),
            smtp_security: Some(crate::config::Security::Starttls),
            username: "test@example.com".into(),
            auth: crate::config::Auth::Password,
            password: Some("secret".into()),
            password_cmd: None,
            token_cmd: None,
            oauth_client_id: None,
            oauth_client_secret: None,
            mail_dir: Some(mail_dir.to_string_lossy().to_string()),
            inbox_folder: None,
            archive_folder: None,
            trash_folder: None,
            sent_folder: None,
            drafts_folder: None,
            label_roots: None,
            storage_mode: None,
            provider: crate::config::Provider::Standard,
            oauth_authorization_url: None,
            oauth_token_url: None,
            oauth_scope: None,
            reject_invalid_certs: None,
            policy: crate::config::MutationPolicy::FullWrite,
        }
    }
}

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
    /// Parse a sync window from optional date strings.
    ///
    /// # Errors
    /// Returns an error if an absolute date is not in YYYY-MM-DD format or
    /// a relative date is unrecognized.
    pub fn parse(since: Option<&str>, before: Option<&str>) -> Result<Self, VivariumError> {
        Ok(Self {
            since: since.map(parse_since).transpose()?,
            before: before.map(parse_absolute_date).transpose()?,
        })
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.since.is_none() && self.before.is_none()
    }

    #[must_use]
    pub fn contains_datetime(&self, date: DateTime<Utc>) -> bool {
        let date = date.date_naive();
        self.since.is_none_or(|since| date >= since)
            && self.before.is_none_or(|before| date < before)
    }
}

/// Sync all folders for an account (IMAP or Proton API).
///
/// # Errors
/// Returns an error if the store cannot be initialized, IMAP/API sync fails,
/// or extraction fails.
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
    let reject_invalid_certs = account.reject_invalid_certs(config) && !insecure;
    let mut result = if limit == Some(0) {
        SyncResult::default()
    } else if matches!(account.provider, Provider::ProtonApi) {
        crate::proton_sync::sync_messages(account, &store, limit, window).await?
    } else {
        crate::imap::sync_messages(account, &store, reject_invalid_certs, limit, window, all)
            .await?
    };
    finish_sync(account, &mut result)?;
    Ok(result)
}

/// Sync only the account's inbound IMAP folder. This seam has no sent-folder,
/// draft, outbox, or remote-mutation authority and is used by watch-inbox.
///
/// # Errors
/// Returns an error if the account uses the Proton API (IMAP-only operation),
/// store initialization fails, IMAP sync fails, or extraction fails.
pub async fn sync_inbox_account(
    account: &Account,
    config: &Config,
    insecure: bool,
    window: SyncWindow,
) -> Result<SyncResult, VivariumError> {
    if matches!(account.provider, Provider::ProtonApi) {
        return Err(VivariumError::Config(format!(
            "account '{}' uses provider = \"proton-api\"; inbound IMAP watch is unavailable",
            account.name
        )));
    }
    let store = MailStore::new(&account.mail_path(config));
    store.ensure_folders()?;
    let reject_invalid_certs = account.reject_invalid_certs(config) && !insecure;
    let mut result =
        crate::imap::sync_inbox_messages(account, &store, reject_invalid_certs, None, window)
            .await?;
    finish_sync(account, &mut result)?;
    Ok(result)
}

fn finish_sync(account: &Account, result: &mut SyncResult) -> Result<(), VivariumError> {
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
    Ok(())
}

/// Reset (delete) the local account cache directory.
///
/// # Errors
/// Returns an error if the path validation fails, the path cannot be resolved,
/// or the directory cannot be removed.
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
    let managed_root = resolved_managed_root(config);
    let is_custom = account.mail_dir.is_some();

    // Resolve the path to a canonical form if it exists. For non-existent
    // paths, resolve the nearest existing ancestor and extend.
    let canonical = resolve_canonical(&mail_path)?;

    validate_reset(&canonical, &managed_root, is_custom, confirm_custom)?;

    if canonical.exists() {
        fs::remove_dir_all(&canonical)?;
    }
    tracing::warn!(
        account = account.name,
        path = %mail_path.display(),
        "reset local account cache"
    );
    Ok(())
}

/// Resolve a path to its canonical form, handling non-existent components
/// by walking up to the nearest existing ancestor and rejoining.
fn resolve_canonical(path: &std::path::Path) -> Result<std::path::PathBuf, VivariumError> {
    if path.exists() {
        return path.canonicalize().map_err(|e| {
            VivariumError::Other(format!("cannot resolve reset path {}: {e}", path.display()))
        });
    }
    // Walk up to the nearest existing ancestor.
    let mut existing = path.to_path_buf();
    let mut tail = std::path::PathBuf::new();
    while !existing.exists() {
        if let Some(name) = existing.file_name() {
            tail = std::path::Path::new(name).join(tail);
            existing = existing
                .parent()
                .ok_or_else(|| VivariumError::Other("reset path has no parent".into()))?
                .to_path_buf();
        } else {
            break;
        }
    }
    let base = existing.canonicalize().map_err(|e| {
        VivariumError::Other(format!(
            "cannot resolve ancestor of reset path {}: {e}",
            path.display()
        ))
    })?;
    Ok(base.join(tail))
}

/// Validate that a resolved canonical path is safe to delete.
///
/// # Containment rule
///
/// - **Managed paths** must be a proper account child under the managed mail
///   root (not the root itself, not an ancestor). The default managed root
///   lives under `$HOME`; managed account children under it are always safe.
/// - **Custom paths** require `confirm_custom = true` AND must be outside
///   home, cwd, repository root, and all their descendants. Must not be or
///   contain the managed mail root. This is fail-closed: any custom path
///   inside a system or repository tree is rejected regardless of confirmation.
fn validate_reset(
    canonical: &std::path::Path,
    managed_root: &std::path::Path,
    is_custom: bool,
    confirm_custom: bool,
) -> Result<(), VivariumError> {
    // Reject root always.
    if canonical == std::path::Path::new("/") {
        return Err(reset_refusal(canonical, "the filesystem root"));
    }

    // Reject mail root itself under any path type.
    if canonical == managed_root {
        return Err(VivariumError::Other(format!(
            "reset target {} is the mail root itself; refusing",
            canonical.display()
        )));
    }

    // Reject any ancestor of the managed root.
    if managed_root.starts_with(canonical) && canonical != managed_root {
        return Err(VivariumError::Other(format!(
            "reset target {} is an ancestor of the mail root {}; refusing",
            canonical.display(),
            managed_root.display()
        )));
    }

    if is_custom {
        reject_dangerous_target(canonical)?;
        return validate_custom_reset(canonical, confirm_custom);
    }

    // Managed paths: validate the root is safe, then check the account child.
    validate_managed_root(managed_root)?;
    validate_managed_reset(canonical, managed_root)
}

fn resolved_managed_root(config: &Config) -> std::path::PathBuf {
    let root = config
        .defaults
        .mail_root
        .as_deref()
        .map_or_else(Config::default_mail_root, crate::config::expand_tilde);
    root.canonicalize().unwrap_or(root)
}

fn reject_dangerous_target(path: &std::path::Path) -> Result<(), VivariumError> {
    let path_str = path.to_string_lossy();
    let lower = path_str.to_ascii_lowercase();

    // Reject root.
    if path == std::path::Path::new("/") || lower == "/" {
        return Err(reset_refusal(path, "the filesystem root"));
    }

    // Reject home, cwd, repository root, and all their descendants.
    let home = dirs::home_dir().unwrap_or_default();
    let cwd = std::env::current_dir().unwrap_or_default();
    let repo_root = find_repo_root(&cwd);
    for dangerous in [&home, &cwd].into_iter().chain(repo_root.iter()) {
        if path == dangerous.as_path() || path.starts_with(dangerous) {
            return Err(reset_refusal(
                path,
                "inside a system or repository directory",
            ));
        }
    }

    // Reject direct parents of home/cwd/repo (dangerous ancestors).
    for dangerous in [&home, &cwd].into_iter().chain(repo_root.iter()) {
        if let Some(parent) = dangerous.parent()
            && path == parent
        {
            return Err(reset_refusal(path, "a parent of a system directory"));
        }
    }

    Ok(())
}

fn reset_refusal(path: &std::path::Path, reason: &str) -> VivariumError {
    VivariumError::Other(format!(
        "reset target {} is {}; refusing to delete",
        path.display(),
        reason
    ))
}

/// Find the nearest enclosing `.git` repository root, if any.
fn find_repo_root(cwd: &std::path::Path) -> Option<std::path::PathBuf> {
    let canonical = cwd.canonicalize().ok()?;
    let mut current = canonical.as_path();
    loop {
        if current.join(".git").exists() {
            return Some(current.to_path_buf());
        }
        current = current.parent()?;
    }
}

fn validate_custom_reset(
    _canonical: &std::path::Path,
    confirm_custom: bool,
) -> Result<(), VivariumError> {
    if !confirm_custom {
        return Err(VivariumError::Other(
            "custom mail_dir requires --confirm-reset to proceed".into(),
        ));
    }
    // System-directory and repository-descendant rejection is handled by
    // reject_dangerous_target in validate_reset before this function is
    // called. The depth-based containment boundary has been replaced by
    // the stricter rule: custom paths must be outside home/cwd/repo entirely.
    Ok(())
}

fn validate_managed_reset(
    canonical: &std::path::Path,
    managed_root: &std::path::Path,
) -> Result<(), VivariumError> {
    if canonical == managed_root {
        return Err(VivariumError::Other(format!(
            "reset target {} is the mail root itself; refusing",
            canonical.display()
        )));
    }
    if !canonical.starts_with(managed_root) {
        return Err(VivariumError::Other(format!(
            "managed reset path {} is outside the mail root {}; refusing",
            canonical.display(),
            managed_root.display()
        )));
    }
    Ok(())
}

/// Validate that the managed mail root itself is safe.
///
/// The default managed root lives under `$HOME` (e.g. `~/.vivarium`) and
/// that is the canonical safe case. Reject roots that ARE root/home/cwd/repo,
/// roots inside cwd/repo, and dangerous ancestors.
fn validate_managed_root(managed_root: &std::path::Path) -> Result<(), VivariumError> {
    // Root is always dangerous.
    if managed_root == std::path::Path::new("/") {
        return Err(reset_refusal(managed_root, "the filesystem root"));
    }

    let home = dirs::home_dir().unwrap_or_default();
    let cwd = std::env::current_dir().unwrap_or_default();
    let repo_root = find_repo_root(&cwd);

    // Reject managed root that IS home, cwd, or repo.
    for dangerous in [&home, &cwd].into_iter().chain(repo_root.iter()) {
        if managed_root == dangerous.as_path() {
            return Err(reset_refusal(
                managed_root,
                "a system or repository directory",
            ));
        }
    }

    // Reject managed roots inside cwd or repo (but allow under home,
    // which is the default case).
    for dangerous in [&cwd].into_iter().chain(repo_root.iter()) {
        if managed_root.starts_with(dangerous) {
            return Err(reset_refusal(managed_root, "inside cwd or a repository"));
        }
    }

    // Reject managed roots that are ancestors of home/cwd/repo.
    // E.g. /Users would contain home and must not be a managed root.
    for dangerous in [&home, &cwd].into_iter().chain(repo_root.iter()) {
        if dangerous.starts_with(managed_root) && managed_root != dangerous {
            return Err(reset_refusal(
                managed_root,
                "an ancestor of a system or repository directory",
            ));
        }
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
#[path = "sync_test.rs"]
mod tests;

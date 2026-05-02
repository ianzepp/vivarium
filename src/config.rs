mod account;
pub mod types;

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

pub use types::{Account, AccountsFile, Auth, Config, Provider, Security};

use crate::error::VivariumError;

#[cfg(test)]
mod tests;

fn config_dir() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".config")
        .join("vivarium")
}

impl Config {
    pub fn load(path: &Path) -> Result<Self, VivariumError> {
        if !path.exists() {
            tracing::debug!(path = %path.display(), "config file not found, using defaults");
            return Ok(Self::default());
        }
        let contents = fs::read_to_string(path).map_err(|e| {
            VivariumError::Config(format!("failed to read {}: {e}", path.display()))
        })?;
        toml::from_str(&contents)
            .map_err(|e| VivariumError::Config(format!("failed to parse config: {e}")))
    }

    pub fn default_path() -> PathBuf {
        config_dir().join("config.toml")
    }

    pub fn default_mail_root() -> PathBuf {
        dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join(".local")
            .join("share")
            .join("vivarium")
    }
}

impl AccountsFile {
    pub fn load(path: &Path) -> Result<Self, VivariumError> {
        Self::load_with_options(path, false)
    }

    pub fn load_with_options(path: &Path, ignore_permissions: bool) -> Result<Self, VivariumError> {
        if !path.exists() {
            return Err(VivariumError::Config(format!(
                "accounts file not found: {}",
                path.display()
            )));
        }
        check_permissions(path, ignore_permissions)?;
        let contents = fs::read_to_string(path).map_err(|e| {
            VivariumError::Config(format!("failed to read {}: {e}", path.display()))
        })?;
        toml::from_str(&contents)
            .map_err(|e| VivariumError::Config(format!("failed to parse accounts: {e}")))
    }

    pub fn default_path() -> PathBuf {
        config_dir().join("accounts.toml")
    }

    pub fn find_account(&self, name: &str) -> Result<&Account, VivariumError> {
        self.accounts
            .iter()
            .find(|a| a.name == name)
            .ok_or_else(|| VivariumError::Config(format!("account not found: {name}")))
    }
}

/// Reject accounts.toml when it is readable by group or others.
fn check_permissions(path: &Path, ignore_permissions: bool) -> Result<(), VivariumError> {
    let metadata = fs::metadata(path)?;
    let mode = metadata.permissions().mode();
    if mode & 0o077 != 0 {
        if ignore_permissions {
            tracing::warn!(
                path = %path.display(),
                mode = format!("{mode:o}"),
                "accounts file has insecure permissions, ignoring by request"
            );
        } else {
            return Err(VivariumError::Config(format!(
                "insecure permissions on {}: expected mode 600, got {mode:o}; rerun with --ignore-permissions to bypass",
                path.display()
            )));
        }
    }
    Ok(())
}

pub fn expand_tilde(path: &str) -> PathBuf {
    if let Some(rest) = path.strip_prefix("~/")
        && let Some(home) = dirs::home_dir()
    {
        return home.join(rest);
    }
    PathBuf::from(path)
}

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::error::VivariumError;

/// General settings from `config.toml`.
#[derive(Debug, Default, Deserialize)]
pub struct Config {
    #[serde(default)]
    pub defaults: Defaults,
}

#[derive(Debug, Default, Deserialize)]
pub struct Defaults {
    /// Base directory for all account mail, e.g. "~/Mail"
    pub mail_root: Option<String>,
    pub check_interval_secs: Option<u64>,
}

/// Credential and connection details from `accounts.toml`.
#[derive(Debug, Deserialize)]
pub struct AccountsFile {
    #[serde(default)]
    pub accounts: Vec<Account>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Account {
    pub name: String,
    pub email: String,
    pub imap_host: String,
    pub imap_port: Option<u16>,
    pub smtp_host: String,
    pub smtp_port: Option<u16>,
    pub username: String,
    pub password: Option<String>,
    pub password_cmd: Option<String>,
    /// Override mail directory for this account
    pub mail_dir: Option<String>,
    /// Provider hint: "gmail" or "standard"
    #[serde(default)]
    pub provider: Provider,
}

#[derive(Debug, Clone, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Provider {
    Gmail,
    #[default]
    Standard,
}

fn config_dir() -> PathBuf {
    let mut path = dirs::config_dir().unwrap_or_else(|| PathBuf::from("."));
    path.push("vivarium");
    path
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
            .join("Mail")
    }
}

impl AccountsFile {
    pub fn load(path: &Path) -> Result<Self, VivariumError> {
        if !path.exists() {
            return Err(VivariumError::Config(format!(
                "accounts file not found: {}",
                path.display()
            )));
        }
        check_permissions(path)?;
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

/// Warn if accounts.toml is readable by group or others.
fn check_permissions(path: &Path) -> Result<(), VivariumError> {
    let metadata = fs::metadata(path)?;
    let mode = metadata.permissions().mode();
    if mode & 0o077 != 0 {
        tracing::warn!(
            path = %path.display(),
            mode = format!("{mode:o}"),
            "accounts file has insecure permissions, expected 600"
        );
    }
    Ok(())
}

impl Account {
    pub async fn resolve_password(&self) -> Result<String, VivariumError> {
        if let Some(ref pw) = self.password {
            return Ok(pw.clone());
        }
        if let Some(ref cmd) = self.password_cmd {
            let output = tokio::process::Command::new("sh")
                .arg("-c")
                .arg(cmd)
                .output()
                .await?;
            if !output.status.success() {
                return Err(VivariumError::Config(format!(
                    "password_cmd failed: {}",
                    String::from_utf8_lossy(&output.stderr)
                )));
            }
            return Ok(String::from_utf8_lossy(&output.stdout).trim().to_string());
        }
        Err(VivariumError::Config(format!(
            "no password or password_cmd for account '{}'",
            self.name
        )))
    }

    /// Resolve the mail directory for this account.
    pub fn mail_path(&self, config: &Config) -> PathBuf {
        if let Some(ref dir) = self.mail_dir {
            return expand_tilde(dir);
        }
        let root = config
            .defaults
            .mail_root
            .as_deref()
            .map(expand_tilde)
            .unwrap_or_else(Config::default_mail_root);
        root.join(&self.name)
    }

    /// Which IMAP folder contains all messages for this provider.
    pub fn all_mail_folder(&self) -> &str {
        match self.provider {
            Provider::Gmail => "[Gmail]/All Mail",
            Provider::Standard => "INBOX",
        }
    }

    /// Which IMAP folder name means "sent" for this provider.
    pub fn sent_folder(&self) -> &str {
        match self.provider {
            Provider::Gmail => "[Gmail]/Sent Mail",
            Provider::Standard => "Sent",
        }
    }

    /// Which IMAP folder name means "drafts" for this provider.
    pub fn drafts_folder(&self) -> &str {
        match self.provider {
            Provider::Gmail => "[Gmail]/Drafts",
            Provider::Standard => "Drafts",
        }
    }
}

pub fn expand_tilde(path: &str) -> PathBuf {
    if let Some(rest) = path.strip_prefix("~/")
        && let Some(home) = dirs::home_dir()
    {
        return home.join(rest);
    }
    PathBuf::from(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn expand_tilde_no_tilde() {
        assert_eq!(expand_tilde("/tmp/mail"), PathBuf::from("/tmp/mail"));
    }

    #[test]
    fn expand_tilde_with_tilde() {
        let expanded = expand_tilde("~/mail");
        assert!(!expanded.starts_with("~"));
    }

    #[test]
    fn config_defaults_when_missing() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("nonexistent.toml");
        let config = Config::load(&path).unwrap();
        assert!(config.defaults.mail_root.is_none());
    }

    #[test]
    fn accounts_file_not_found() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("nonexistent.toml");
        let err = AccountsFile::load(&path).unwrap_err();
        assert!(err.to_string().contains("not found"));
    }

    #[test]
    fn accounts_file_parses() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("accounts.toml");
        fs::write(&path, r#"
            [[accounts]]
            name = "test"
            email = "test@example.com"
            imap_host = "imap.example.com"
            smtp_host = "smtp.example.com"
            username = "test"
            password = "secret"
        "#).unwrap();
        // Set secure permissions
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
        let accounts = AccountsFile::load(&path).unwrap();
        assert_eq!(accounts.accounts.len(), 1);
        assert_eq!(accounts.accounts[0].name, "test");
    }

    #[test]
    fn find_account_by_name() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("accounts.toml");
        fs::write(&path, r#"
            [[accounts]]
            name = "gmail"
            email = "ian@gmail.com"
            imap_host = "imap.gmail.com"
            smtp_host = "smtp.gmail.com"
            username = "ian"
            password = "pw"
            provider = "gmail"

            [[accounts]]
            name = "proton"
            email = "ian@proton.me"
            imap_host = "127.0.0.1"
            smtp_host = "127.0.0.1"
            username = "ian"
            password = "pw"
        "#).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
        let accounts = AccountsFile::load(&path).unwrap();
        let gmail = accounts.find_account("gmail").unwrap();
        assert_eq!(gmail.provider, Provider::Gmail);
        let proton = accounts.find_account("proton").unwrap();
        assert_eq!(proton.provider, Provider::Standard);
        assert!(accounts.find_account("nope").is_err());
    }
}

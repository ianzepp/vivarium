use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::error::VivariumError;

/// General settings from `config.toml`.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct Config {
    #[serde(default)]
    pub defaults: Defaults,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct Defaults {
    /// Base directory for all account mail, e.g. "~/Mail"
    pub mail_root: Option<String>,
    pub check_interval_secs: Option<u64>,
    #[serde(default)]
    pub reject_invalid_certs: bool,
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
    #[serde(default)]
    pub imap_security: Security,
    pub smtp_host: String,
    pub smtp_port: Option<u16>,
    #[serde(default)]
    pub smtp_security: Security,
    pub username: String,
    #[serde(default)]
    pub auth: Auth,
    pub password: Option<String>,
    pub password_cmd: Option<String>,
    pub token_cmd: Option<String>,
    pub oauth_client_id: Option<String>,
    pub oauth_client_secret: Option<String>,
    /// Override mail directory for this account
    pub mail_dir: Option<String>,
    /// Provider hint: "gmail", "protonmail", or "standard"
    #[serde(default)]
    pub provider: Provider,
    /// OAuth authorization endpoint (overrides provider defaults)
    pub oauth_authorization_url: Option<String>,
    /// OAuth token exchange endpoint (overrides provider defaults)
    pub oauth_token_url: Option<String>,
    /// OAuth scope(s) (overrides provider defaults)
    pub oauth_scope: Option<String>,
    pub reject_invalid_certs: Option<bool>,
}

#[derive(Debug, Clone, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Provider {
    Gmail,
    Protonmail,
    #[default]
    Standard,
}

impl std::fmt::Display for Provider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Provider::Gmail => write!(f, "gmail"),
            Provider::Protonmail => write!(f, "protonmail"),
            Provider::Standard => write!(f, "standard"),
        }
    }
}

/// OAuth endpoints for a known provider.
#[derive(Debug)]
pub struct ProviderOAuthConfig {
    pub auth_url: String,
    pub token_url: String,
    pub scope: String,
}

impl Provider {
    /// Return the OAuth configuration for this provider.
    pub fn oauth_config(&self) -> Option<ProviderOAuthConfig> {
        match self {
            Provider::Gmail => Some(ProviderOAuthConfig {
                auth_url: "https://accounts.google.com/o/oauth2/v2/auth".into(),
                token_url: "https://oauth2.googleapis.com/token".into(),
                scope: "https://mail.google.com/".into(),
            }),
            Provider::Protonmail => Some(ProviderOAuthConfig {
                auth_url: "https://account.proton.me/oauth".into(),
                token_url: "https://account.proton.me/token".into(),
                scope: "https://mail.protonmail.com/wildcard".into(),
            }),
            Provider::Standard => None,
        }
    }
}

#[derive(Debug, Clone, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Security {
    /// Direct TLS/SSL connection (port 993 for IMAP, 465 for SMTP)
    #[default]
    Ssl,
    /// STARTTLS upgrade from plaintext (port 143 for IMAP, 587 for SMTP)
    Starttls,
}

#[derive(Debug, Clone, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Auth {
    #[default]
    Password,
    Xoauth2,
}

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

impl Account {
    pub async fn resolve_secret(&self) -> Result<String, VivariumError> {
        match self.auth {
            Auth::Password => self.resolve_password().await,
            Auth::Xoauth2 => self.resolve_oauth_token().await,
        }
    }

    async fn resolve_password(&self) -> Result<String, VivariumError> {
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

    async fn resolve_oauth_token(&self) -> Result<String, VivariumError> {
        let Some(ref cmd) = self.token_cmd else {
            return Err(VivariumError::Config(format!(
                "auth = \"xoauth2\" requires token_cmd for account '{}'",
                self.name
            )));
        };

        let output = tokio::process::Command::new("sh")
            .arg("-c")
            .arg(cmd)
            .output()
            .await?;
        if !output.status.success() {
            return Err(VivariumError::Config(format!(
                "token_cmd failed: {}",
                String::from_utf8_lossy(&output.stderr)
            )));
        }

        let token = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if token.is_empty() {
            return Err(VivariumError::Config(format!(
                "token_cmd produced an empty token for account '{}'",
                self.name
            )));
        }
        Ok(token)
    }

    pub fn reject_invalid_certs(&self, config: &Config) -> bool {
        self.reject_invalid_certs
            .unwrap_or(config.defaults.reject_invalid_certs)
    }

    /// Resolve the mail directory for this account.
    /// Resolve OAuth URLs: account-level overrides take priority, then provider defaults.
    pub fn oauth_urls(&self) -> Result<ProviderOAuthConfig, VivariumError> {
        if let (Some(auth), Some(token), Some(scope)) = (
            &self.oauth_authorization_url,
            &self.oauth_token_url,
            &self.oauth_scope,
        ) {
            return Ok(ProviderOAuthConfig {
                auth_url: auth.clone(),
                token_url: token.clone(),
                scope: scope.clone(),
            });
        }

        if let Some(provider_config) = self.provider.oauth_config() {
            return Ok(provider_config);
        }

        Err(VivariumError::Config(format!(
            "account '{}' has auth=xoauth2 but provider={} has no OAuth defaults; \
             set oauth_authorization_url, oauth_token_url, and oauth_scope \
             in accounts.toml",
            self.name, self.provider
        )))
    }

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
            Provider::Standard | Provider::Protonmail => "INBOX",
        }
    }

    /// Which IMAP folder name means "sent" for this provider.
    pub fn sent_folder(&self) -> &str {
        match self.provider {
            Provider::Gmail => "[Gmail]/Sent Mail",
            Provider::Standard | Provider::Protonmail => "Sent",
        }
    }

    /// Which IMAP folder name means "drafts" for this provider.
    pub fn drafts_folder(&self) -> &str {
        match self.provider {
            Provider::Gmail => "[Gmail]/Drafts",
            Provider::Standard | Provider::Protonmail => "Drafts",
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
        assert!(!config.defaults.reject_invalid_certs);
    }

    #[test]
    fn config_parses_reject_invalid_certs_default() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("config.toml");
        fs::write(
            &path,
            r#"
            [defaults]
            reject_invalid_certs = true
        "#,
        )
        .unwrap();

        let config = Config::load(&path).unwrap();
        assert!(config.defaults.reject_invalid_certs);
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
        fs::write(
            &path,
            r#"
            [[accounts]]
            name = "test"
            email = "test@example.com"
            imap_host = "imap.example.com"
            smtp_host = "smtp.example.com"
            username = "test"
            password = "secret"
        "#,
        )
        .unwrap();
        // Set secure permissions
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
        let accounts = AccountsFile::load(&path).unwrap();
        assert_eq!(accounts.accounts.len(), 1);
        assert_eq!(accounts.accounts[0].name, "test");
    }

    #[test]
    fn accounts_file_rejects_insecure_permissions() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("accounts.toml");
        fs::write(
            &path,
            r#"
            [[accounts]]
            name = "test"
            email = "test@example.com"
            imap_host = "imap.example.com"
            smtp_host = "smtp.example.com"
            username = "test"
            password = "secret"
        "#,
        )
        .unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).unwrap();

        let err = AccountsFile::load(&path).unwrap_err();
        assert!(err.to_string().contains("insecure permissions"));
    }

    #[test]
    fn accounts_file_can_ignore_insecure_permissions() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("accounts.toml");
        fs::write(
            &path,
            r#"
            [[accounts]]
            name = "test"
            email = "test@example.com"
            imap_host = "imap.example.com"
            smtp_host = "smtp.example.com"
            username = "test"
            password = "secret"
        "#,
        )
        .unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).unwrap();

        let accounts = AccountsFile::load_with_options(&path, true).unwrap();
        assert_eq!(accounts.accounts.len(), 1);
    }

    #[test]
    fn account_reject_invalid_certs_overrides_default() {
        let config = Config {
            defaults: Defaults {
                reject_invalid_certs: true,
                ..Defaults::default()
            },
        };
        let account = Account {
            name: "test".into(),
            email: "test@example.com".into(),
            imap_host: "imap.example.com".into(),
            imap_port: None,
            imap_security: Security::Ssl,
            smtp_host: "smtp.example.com".into(),
            smtp_port: None,
            smtp_security: Security::Ssl,
            username: "test".into(),
            auth: Auth::Password,
            password: Some("secret".into()),
            password_cmd: None,
            token_cmd: None,
            oauth_client_id: None,
            oauth_client_secret: None,
            oauth_authorization_url: None,
            oauth_token_url: None,
            oauth_scope: None,
            mail_dir: None,
            provider: Provider::Standard,
            reject_invalid_certs: Some(false),
        };

        assert!(!account.reject_invalid_certs(&config));
    }

    #[test]
    fn find_account_by_name() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("accounts.toml");
        fs::write(
            &path,
            r#"
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
        "#,
        )
        .unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
        let accounts = AccountsFile::load(&path).unwrap();
        let gmail = accounts.find_account("gmail").unwrap();
        assert_eq!(gmail.provider, Provider::Gmail);
        assert_eq!(gmail.auth, Auth::Password);
        let proton = accounts.find_account("proton").unwrap();
        assert_eq!(proton.provider, Provider::Standard);
        assert!(accounts.find_account("nope").is_err());
    }

    #[test]
    fn account_parses_xoauth2_auth() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("accounts.toml");
        fs::write(
            &path,
            r#"
            [[accounts]]
            name = "gmail"
            email = "ian@gmail.com"
            imap_host = "imap.gmail.com"
            smtp_host = "smtp.gmail.com"
            username = "ian@gmail.com"
            auth = "xoauth2"
            provider = "gmail"
            token_cmd = "pass gmail-token"
            oauth_client_id = "client-id"
            oauth_client_secret = "client-secret"
        "#,
        )
        .unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();

        let accounts = AccountsFile::load(&path).unwrap();
        let gmail = accounts.find_account("gmail").unwrap();
        assert_eq!(gmail.auth, Auth::Xoauth2);
        assert_eq!(gmail.token_cmd.as_deref(), Some("pass gmail-token"));
        assert_eq!(gmail.oauth_client_id.as_deref(), Some("client-id"));
        // Gmail has built-in OAuth defaults
        let urls = gmail.oauth_urls().unwrap();
        assert!(urls.auth_url.contains("google"));
        assert!(urls.token_url.contains("google"));
        assert!(urls.scope.contains("mail.google"));
    }

    #[test]
    fn provider_standard_has_no_oauth_defaults() {
        assert!(Provider::Standard.oauth_config().is_none());
    }

    #[test]
    fn provider_protonmail_has_oauth_defaults() {
        let config = Provider::Protonmail.oauth_config().unwrap();
        assert!(config.auth_url.contains("proton"));
        assert!(config.token_url.contains("proton"));
        assert!(config.scope.contains("protonmail"));
    }

    #[test]
    fn account_oauth_urls_override_provider_defaults() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("accounts.toml");
        fs::write(
            &path,
            r#"
            [[accounts]]
            name = "custom"
            email = "user@example.com"
            imap_host = "imap.example.com"
            smtp_host = "smtp.example.com"
            username = "user"
            auth = "xoauth2"
            provider = "standard"
            oauth_authorization_url = "https://custom.auth/authorize"
            oauth_token_url = "https://custom.auth/token"
            oauth_scope = "https://custom.auth/mail"
        "#,
        )
        .unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();

        let accounts = AccountsFile::load(&path).unwrap();
        let acct = accounts.find_account("custom").unwrap();
        let urls = acct.oauth_urls().unwrap();
        assert!(urls.auth_url.contains("custom.auth/authorize"));
        assert!(urls.token_url.contains("custom.auth/token"));
        assert!(urls.scope.contains("custom.auth/mail"));
    }

    #[test]
    fn account_xoauth2_standard_provider_errors_without_urls() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("accounts.toml");
        fs::write(
            &path,
            r#"
            [[accounts]]
            name = "no-provider"
            email = "user@example.com"
            imap_host = "imap.example.com"
            smtp_host = "smtp.example.com"
            username = "user"
            auth = "xoauth2"
            provider = "standard"
        "#,
        )
        .unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();

        let accounts = AccountsFile::load(&path).unwrap();
        let acct = accounts.find_account("no-provider").unwrap();
        let err = acct.oauth_urls().unwrap_err();
        assert!(err.to_string().contains("no OAuth defaults"));
    }
}

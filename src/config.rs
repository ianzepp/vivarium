mod account;
pub mod types;

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

pub use types::{Account, AccountsFile, Auth, Config, Provider, Security};

use crate::error::VivariumError;
use types::ProviderOAuthConfig;

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
        fs::write(&path, "[defaults]\nreject_invalid_certs = true\n").unwrap();
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
        fs::write(&path, "[[accounts]]\nname=\"t\"\nemail=\"e\"\nimap_host=\"h\"\nsmtp_host=\"s\"\nusername=\"u\"\npassword=\"p\"\n").unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).unwrap();
        let accounts = AccountsFile::load_with_options(&path, true).unwrap();
        assert_eq!(accounts.accounts.len(), 1);
    }

    #[test]
    fn find_account_by_name() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("accounts.toml");
        fs::write(&path, "[[accounts]]\nname = \"a\"\nemail = \"a@b\"\nimap_host = \"h\"\nsmtp_host = \"s\"\nusername = \"u\"\npassword = \"p\"\n").unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
        let accounts = AccountsFile::load(&path).unwrap();
        assert_eq!(accounts.find_account("a").unwrap().name, "a");
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
        assert_eq!(gmail.auth, types::Auth::Xoauth2);
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
        assert!(types::Provider::Standard.oauth_config().is_none());
    }

    #[test]
    fn provider_protonmail_has_oauth_defaults() {
        let config = types::Provider::Protonmail.oauth_config().unwrap();
        assert!(config.auth_url.contains("proton"));
        assert!(config.token_url.contains("proton"));
        assert!(config.scope.contains("protonmail"));
    }

    #[test]
    fn account_oauth_urls_override_provider_defaults() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("accounts.toml");
        fs::write(&path, r#"[[accounts]]
name="c"
email="u@e.com"
imap_host="imap.e.com"
smtp_host="smtp.e.com"
username="u"
auth="xoauth2"
provider="standard"
oauth_authorization_url="https://custom.auth/authorize"
oauth_token_url="https://custom.auth/token"
oauth_scope="https://custom.auth/mail"
"#).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
        let accounts = AccountsFile::load(&path).unwrap();
        let acct = accounts.find_account("c").unwrap();
        let urls = acct.oauth_urls().unwrap();
        assert!(urls.auth_url.contains("custom.auth/authorize"));
        assert!(urls.token_url.contains("custom.auth/token"));
        assert!(urls.scope.contains("custom.auth/mail"));
    }

    #[test]
    fn account_xoauth2_standard_provider_errors_without_urls() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("accounts.toml");
        fs::write(&path, "[[accounts]]\nname=\"n\"\nemail=\"u@e\"\nimap_host=\"h\"\nsmtp_host=\"s\"\nusername=\"u\"\nauth=\"xoauth2\"\nprovider=\"standard\"\n").unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
        let accounts = AccountsFile::load(&path).unwrap();
        let acct = accounts.find_account("n").unwrap();
        assert!(acct.oauth_urls().is_err());
    }

    #[test]
    fn protonmail_defaults_resolved_host_when_empty() {
        let account = types::Account {
            name: "proton".into(), email: "u@p.me".into(), imap_host: "".into(),
            imap_port: None, imap_security: types::Security::Ssl,
            smtp_host: "".into(), smtp_port: None, smtp_security: types::Security::Ssl,
            username: "u".into(), auth: types::Auth::Password, password: Some("pw".into()),
            password_cmd: None, token_cmd: None, oauth_client_id: None,
            oauth_client_secret: None, oauth_authorization_url: None,
            oauth_token_url: None, oauth_scope: None, mail_dir: None,
            provider: types::Provider::Protonmail, reject_invalid_certs: None,
        };
        assert_eq!(account.resolved_imap_host(), "127.0.0.1");
        assert_eq!(account.resolved_imap_port(), 1143);
        assert_eq!(account.resolved_smtp_port(), 1025);
        assert!(account.defaults_to_accept_invalid_certs());
    }

    #[test]
    fn protonmail_defaults_resolves_explicit_host() {
        let account = types::Account {
            name: "proton".into(), email: "u@p.me".into(), imap_host: "bridge.local".into(),
            imap_port: None, imap_security: types::Security::Starttls,
            smtp_host: "".into(), smtp_port: None, smtp_security: types::Security::Ssl,
            username: "u".into(), auth: types::Auth::Password, password: Some("pw".into()),
            password_cmd: None, token_cmd: None, oauth_client_id: None,
            oauth_client_secret: None, oauth_authorization_url: None,
            oauth_token_url: None, oauth_scope: None, mail_dir: None,
            provider: types::Provider::Protonmail, reject_invalid_certs: None,
        };
        assert_eq!(account.resolved_imap_host(), "bridge.local");
        assert_eq!(account.resolved_imap_port(), 1143);
        assert_eq!(account.resolved_smtp_port(), 1025);
    }

    #[test]
    fn protonmail_default_reject_invalid_certs_is_true() {
        let account = types::Account {
            name: "proton".into(), email: "u@p.me".into(), imap_host: "127.0.0.1".into(),
            imap_port: None, imap_security: types::Security::Ssl,
            smtp_host: "127.0.0.1".into(), smtp_port: None, smtp_security: types::Security::Ssl,
            username: "u".into(), auth: types::Auth::Password, password: Some("pw".into()),
            password_cmd: None, token_cmd: None, oauth_client_id: None,
            oauth_client_secret: None, oauth_authorization_url: None,
            oauth_token_url: None, oauth_scope: None, mail_dir: None,
            provider: types::Provider::Protonmail, reject_invalid_certs: None,
        };
        let config = Config::default();
        assert!(account.reject_invalid_certs(&config));
    }

    #[test]
    fn standard_provider_uses_config_default_for_certs() {
        let config = Config {
            defaults: types::Defaults { reject_invalid_certs: true, ..types::Defaults::default() },
        };
        let account = types::Account {
            name: "custom".into(), email: "u@e.com".into(), imap_host: "imap.e.com".into(),
            imap_port: None, imap_security: types::Security::Ssl,
            smtp_host: "smtp.e.com".into(), smtp_port: None, smtp_security: types::Security::Ssl,
            username: "u".into(), auth: types::Auth::Password, password: Some("pw".into()),
            password_cmd: None, token_cmd: None, oauth_client_id: None,
            oauth_client_secret: None, oauth_authorization_url: None,
            oauth_token_url: None, oauth_scope: None, mail_dir: None,
            provider: types::Provider::Standard, reject_invalid_certs: None,
        };
        assert!(account.reject_invalid_certs(&config));
    }
}

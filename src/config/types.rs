use serde::Deserialize;

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
    pub inbox_folder: Option<String>,
    pub archive_folder: Option<String>,
    pub trash_folder: Option<String>,
    pub sent_folder: Option<String>,
    pub drafts_folder: Option<String>,
    pub label_roots: Option<Vec<String>>,
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

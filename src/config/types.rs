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
    /// Optional embedding provider name for semantic indexing/search.
    pub embedding_provider: Option<String>,
    /// Optional embedding model name for semantic indexing/search.
    pub embedding_model: Option<String>,
    /// Optional embedding endpoint URL for semantic indexing/search.
    pub embedding_endpoint: Option<String>,
    /// Local document-rendering policy.
    #[serde(default)]
    pub render: RenderDefaults,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RenderDefaults {
    /// Pinned renderer name, such as `pandoc-tectonic` or `pandoc-html`.
    pub engine: Option<String>,
    /// Renderer names that auto-selection and pins must reject.
    #[serde(default)]
    pub deny_engines: Vec<String>,
    /// Whether auto-selection may try the next safe installed pipeline.
    #[serde(default = "default_true")]
    pub allow_fallback: bool,
}

fn default_true() -> bool {
    true
}

impl Default for RenderDefaults {
    fn default() -> Self {
        Self {
            engine: None,
            deny_engines: Vec::new(),
            allow_fallback: true,
        }
    }
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
    #[serde(default)]
    pub imap_host: String,
    pub imap_port: Option<u16>,
    pub imap_security: Option<Security>,
    #[serde(default)]
    pub smtp_host: String,
    pub smtp_port: Option<u16>,
    pub smtp_security: Option<Security>,
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
    pub storage_mode: Option<StorageMode>,
    /// Provider hint: "gmail", "proton-api", "protonmail", or "standard"
    #[serde(default)]
    pub provider: Provider,
    /// OAuth authorization endpoint (overrides provider defaults)
    pub oauth_authorization_url: Option<String>,
    /// OAuth token exchange endpoint (overrides provider defaults)
    pub oauth_token_url: Option<String>,
    /// OAuth scope(s) (overrides provider defaults)
    pub oauth_scope: Option<String>,
    pub reject_invalid_certs: Option<bool>,
    /// Account mutation policy: controls which remote side effects are authorized.
    #[serde(default)]
    pub policy: MutationPolicy,
}

#[derive(Debug, Clone, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Provider {
    Gmail,
    #[serde(rename = "proton-api")]
    ProtonApi,
    Protonmail,
    #[default]
    Standard,
}

impl std::fmt::Display for Provider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Provider::Gmail => write!(f, "gmail"),
            Provider::ProtonApi => write!(f, "proton-api"),
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
    #[must_use] 
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
            Provider::ProtonApi | Provider::Standard => None,
        }
    }
}

#[derive(Debug, Clone, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Security {
    /// Implicit TLS/SSL connection (port 993 for IMAP, 465 for SMTP).
    #[default]
    Ssl,
    /// STARTTLS upgrade from plaintext (port 143 for IMAP, 587 for SMTP).
    Starttls,
}

#[derive(Debug, Clone, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Auth {
    #[default]
    Password,
    Xoauth2,
}

impl std::fmt::Display for Auth {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Auth::Password => write!(f, "password"),
            Auth::Xoauth2 => write!(f, "xoauth2"),
        }
    }
}

#[derive(Debug, Clone, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum StorageMode {
    /// Do not keep a durable local cache. Live proxy commands are intentionally limited.
    Proxy,
    /// Store headers and metadata only. Bodies are fetched only by future on-demand paths.
    #[default]
    Headers,
    /// Store full messages locally, without building semantic embeddings by default.
    Bodies,
    /// Store full messages locally and allow semantic body embedding/indexing.
    Semantic,
}

impl std::fmt::Display for StorageMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            StorageMode::Proxy => write!(f, "proxy"),
            StorageMode::Headers => write!(f, "headers"),
            StorageMode::Bodies => write!(f, "bodies"),
            StorageMode::Semantic => write!(f, "semantic"),
        }
    }
}

/// Account mutation policy. Controls which remote side effects the selected
/// account is authorized to perform. Existing accounts default to `FullWrite`,
/// preserving current behavior.
#[derive(Debug, Clone, Copy, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum MutationPolicy {
    /// All remote operations: archive, move, trash, delete, expunge, flag, send.
    #[default]
    FullWrite,
    /// Sync/read/search/show only. Denies all remote mutations and send.
    ReadOnly,
    /// Read-mostly: archive, non-trash moves, flags. Denies move-to-trash,
    /// delete/trash, expunge, and send.
    Archive,
}

impl std::fmt::Display for MutationPolicy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MutationPolicy::FullWrite => write!(f, "full-write"),
            MutationPolicy::ReadOnly => write!(f, "read-only"),
            MutationPolicy::Archive => write!(f, "archive"),
        }
    }
}

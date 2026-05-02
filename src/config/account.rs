use std::path::PathBuf;

use super::expand_tilde;
use super::types::{Account, Auth, Config, Provider, ProviderOAuthConfig, Security};
use crate::error::VivariumError;

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
        self.reject_invalid_certs.unwrap_or(match self.provider {
            Provider::Protonmail => true,
            _ => config.defaults.reject_invalid_certs,
        })
    }

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

    /// Provider-specific upstream aggregate used for sync only.
    pub fn all_mail_folder(&self) -> &str {
        match self.provider {
            crate::config::types::Provider::Gmail => "[Gmail]/All Mail",
            crate::config::types::Provider::Protonmail => "All Mail",
            crate::config::types::Provider::Standard => "INBOX",
        }
    }

    pub fn inbox_folder(&self) -> String {
        self.inbox_folder.clone().unwrap_or_else(|| "INBOX".into())
    }

    pub fn archive_folder(&self) -> String {
        self.archive_folder
            .clone()
            .unwrap_or_else(|| "Archive".into())
    }

    pub fn trash_folder(&self) -> String {
        self.trash_folder
            .clone()
            .unwrap_or_else(|| match self.provider {
                crate::config::types::Provider::Gmail => "[Gmail]/Trash".into(),
                crate::config::types::Provider::Standard
                | crate::config::types::Provider::Protonmail => "Trash".into(),
            })
    }

    pub fn sent_folder(&self) -> String {
        self.sent_folder.clone().unwrap_or_else(|| {
            match self.provider {
                crate::config::types::Provider::Gmail => "[Gmail]/Sent Mail",
                crate::config::types::Provider::Standard
                | crate::config::types::Provider::Protonmail => "Sent",
            }
            .into()
        })
    }

    pub fn drafts_folder(&self) -> String {
        self.drafts_folder.clone().unwrap_or_else(|| {
            match self.provider {
                crate::config::types::Provider::Gmail => "[Gmail]/Drafts",
                crate::config::types::Provider::Standard
                | crate::config::types::Provider::Protonmail => "Drafts",
            }
            .into()
        })
    }

    pub fn label_roots(&self) -> Vec<String> {
        self.label_roots.clone().unwrap_or_default()
    }

    /// Resolved IMAP host, with provider defaults applied.
    pub fn resolved_imap_host(&self) -> String {
        if !self.imap_host.is_empty() {
            return self.imap_host.clone();
        }
        match self.provider {
            crate::config::types::Provider::Protonmail => "127.0.0.1".into(),
            _ => self.imap_host.clone(),
        }
    }

    /// Resolved IMAP port, with provider defaults applied.
    pub fn resolved_imap_port(&self) -> u16 {
        if let Some(port) = self.imap_port {
            return port;
        }
        match self.provider {
            Provider::Protonmail => 1143,
            _ => match self.imap_security {
                Security::Ssl => 993,
                Security::Starttls => 143,
            },
        }
    }

    /// Resolved SMTP host, with provider defaults applied.
    pub fn resolved_smtp_host(&self) -> String {
        if !self.smtp_host.is_empty() {
            return self.smtp_host.clone();
        }
        match self.provider {
            Provider::Protonmail => "127.0.0.1".into(),
            _ => self.smtp_host.clone(),
        }
    }

    /// Resolved SMTP port, with provider defaults applied.
    pub fn resolved_smtp_port(&self) -> u16 {
        if let Some(port) = self.smtp_port {
            return port;
        }
        match self.provider {
            Provider::Protonmail => 1025,
            _ => match self.smtp_security {
                Security::Ssl => 465,
                Security::Starttls => 587,
            },
        }
    }

    /// Whether this account should accept self-signed certificates by default.
    pub fn defaults_to_accept_invalid_certs(&self) -> bool {
        matches!(self.provider, crate::config::types::Provider::Protonmail)
    }
}

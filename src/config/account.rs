use std::path::PathBuf;

use super::{Account, Auth, Config, ProviderOAuthConfig, expand_tilde};
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
        self.reject_invalid_certs
            .unwrap_or(config.defaults.reject_invalid_certs)
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

    /// Which IMAP folder contains all messages for this provider.
    pub fn all_mail_folder(&self) -> &str {
        match self.provider {
            crate::config::types::Provider::Gmail => "[Gmail]/All Mail",
            crate::config::types::Provider::Standard
            | crate::config::types::Provider::Protonmail => "INBOX",
        }
    }

    /// Which IMAP folder name means "sent" for this provider.
    pub fn sent_folder(&self) -> &str {
        match self.provider {
            crate::config::types::Provider::Gmail => "[Gmail]/Sent Mail",
            crate::config::types::Provider::Standard
            | crate::config::types::Provider::Protonmail => "Sent",
        }
    }

    /// Which IMAP folder name means "drafts" for this provider.
    pub fn drafts_folder(&self) -> &str {
        match self.provider {
            crate::config::types::Provider::Gmail => "[Gmail]/Drafts",
            crate::config::types::Provider::Standard
            | crate::config::types::Provider::Protonmail => "Drafts",
        }
    }
}

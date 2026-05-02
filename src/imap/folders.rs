use async_imap::types::{Capability, NameAttribute};
use futures::TryStreamExt;
use serde::Serialize;

use super::transport::connect;
use crate::config::Account;
use crate::error::VivariumError;

#[derive(Debug, Clone, Serialize)]
pub struct FolderDiscovery {
    pub account: String,
    pub provider: String,
    pub resolved: ResolvedFolders,
    pub capabilities: CapabilityReport,
    pub folders: Vec<RemoteFolder>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ResolvedFolders {
    pub inbox: String,
    pub archive: String,
    pub trash: String,
    pub sent: String,
    pub drafts: String,
    pub label_roots: Vec<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct CapabilityReport {
    pub uidplus: bool,
    pub move_supported: bool,
    pub special_use: bool,
    pub append: bool,
    pub idle: bool,
    pub raw: Vec<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct RemoteFolder {
    pub name: String,
    pub delimiter: Option<String>,
    pub attributes: Vec<String>,
}

pub async fn discover_folders(
    account: &Account,
    reject_invalid_certs: bool,
) -> Result<FolderDiscovery, VivariumError> {
    let mut session = connect(account, reject_invalid_certs).await?;
    let caps = session
        .capabilities()
        .await
        .map_err(|e| VivariumError::Imap(format!("CAPABILITY failed: {e}")))?;
    let capabilities = capability_report(&caps);
    let names = session
        .list(Some(""), Some("*"))
        .await
        .map_err(|e| VivariumError::Imap(format!("LIST failed: {e}")))?;
    let folders = names
        .map_ok(|name| RemoteFolder {
            name: name.name().to_string(),
            delimiter: name.delimiter().map(str::to_string),
            attributes: name.attributes().iter().map(attribute_name).collect(),
        })
        .try_collect()
        .await
        .map_err(|e| VivariumError::Imap(format!("LIST stream failed: {e}")))?;
    session.logout().await.ok();

    Ok(FolderDiscovery {
        account: account.name.clone(),
        provider: account.provider.to_string(),
        resolved: resolved_folders(account),
        capabilities,
        folders,
    })
}

pub fn resolved_folders(account: &Account) -> ResolvedFolders {
    ResolvedFolders {
        inbox: account.inbox_folder(),
        archive: account.archive_folder(),
        trash: account.trash_folder(),
        sent: account.sent_folder(),
        drafts: account.drafts_folder(),
        label_roots: account.label_roots(),
    }
}

fn capability_report(caps: &async_imap::types::Capabilities) -> CapabilityReport {
    let mut raw = caps.iter().map(capability_name).collect::<Vec<_>>();
    raw.sort();
    CapabilityReport {
        uidplus: caps.has_str("UIDPLUS"),
        move_supported: caps.has_str("MOVE"),
        special_use: caps.has_str("SPECIAL-USE"),
        append: caps.has_str("APPEND") || caps.has_str("IMAP4rev1"),
        idle: caps.has_str("IDLE"),
        raw,
    }
}

fn capability_name(capability: &Capability) -> String {
    match capability {
        Capability::Imap4rev1 => "IMAP4rev1".into(),
        Capability::Auth(value) => format!("AUTH={value}"),
        Capability::Atom(value) => value.clone(),
    }
}

fn attribute_name(attribute: &NameAttribute<'_>) -> String {
    format!("{attribute:?}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{Auth, Provider, Security};

    #[test]
    fn resolves_provider_folder_defaults() {
        let proton = account(Provider::Protonmail);
        let gmail = account(Provider::Gmail);
        let standard = account(Provider::Standard);

        assert_eq!(resolved_folders(&proton).archive, "All Mail");
        assert_eq!(resolved_folders(&proton).trash, "Trash");
        assert_eq!(resolved_folders(&gmail).archive, "[Gmail]/All Mail");
        assert_eq!(resolved_folders(&gmail).sent, "[Gmail]/Sent Mail");
        assert_eq!(resolved_folders(&standard).archive, "INBOX");
    }

    #[test]
    fn resolves_account_folder_overrides() {
        let mut account = account(Provider::Standard);
        account.archive_folder = Some("Archive".into());
        account.trash_folder = Some("Deleted Messages".into());
        account.label_roots = Some(vec!["Labels".into()]);

        let resolved = resolved_folders(&account);

        assert_eq!(resolved.archive, "Archive");
        assert_eq!(resolved.trash, "Deleted Messages");
        assert_eq!(resolved.label_roots, vec!["Labels"]);
    }

    fn account(provider: Provider) -> Account {
        Account {
            name: "test".into(),
            email: "test@example.com".into(),
            imap_host: "localhost".into(),
            imap_port: Some(1143),
            imap_security: Security::Starttls,
            smtp_host: "localhost".into(),
            smtp_port: Some(1025),
            smtp_security: Security::Starttls,
            username: "test@example.com".into(),
            auth: Auth::Password,
            password: Some("secret".into()),
            password_cmd: None,
            token_cmd: None,
            oauth_client_id: None,
            oauth_client_secret: None,
            mail_dir: None,
            inbox_folder: None,
            archive_folder: None,
            trash_folder: None,
            sent_folder: None,
            drafts_folder: None,
            label_roots: None,
            provider,
            oauth_authorization_url: None,
            oauth_token_url: None,
            oauth_scope: None,
            reject_invalid_certs: None,
        }
    }
}

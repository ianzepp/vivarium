use std::fs;
use std::path::{Path, PathBuf};

use notify::{Event, EventKind, RecursiveMode, Watcher};

use crate::config::Account;
use crate::error::VivariumError;
use crate::policy::{self, RemoteMutation};
use crate::store::{MailStore, message_id_from_path};

pub async fn watch_outbox(
    account: &Account,
    store: &MailStore,
    reject_invalid_certs: bool,
) -> Result<(), VivariumError> {
    let outbox_new = store.folder_path("outbox").join("new");
    fs::create_dir_all(&outbox_new)?;

    let (tx, mut rx) = tokio::sync::mpsc::channel(64);
    let mut watcher = notify::recommended_watcher(move |event| {
        if tx.blocking_send(event).is_err() {
            tracing::debug!("outbox watch receiver closed");
        }
    })
    .map_err(|e| VivariumError::Other(format!("failed to create outbox watcher: {e}")))?;

    watcher
        .watch(&outbox_new, RecursiveMode::NonRecursive)
        .map_err(|e| VivariumError::Other(format!("failed to watch outbox: {e}")))?;

    tracing::info!(path = %outbox_new.display(), "watching outbox");
    while let Some(event) = rx.recv().await {
        match event {
            Ok(event) => {
                tracing::debug!(event = ?event, "outbox event");
                for path in dispatchable_paths(&event) {
                    if let Err(err) =
                        process_entry(account, store, &path, reject_invalid_certs).await
                    {
                        tracing::warn!(path = %path.display(), error = %err, "outbox dispatch failed");
                    }
                }
            }
            Err(err) => tracing::warn!(error = %err, "outbox watch error"),
        }
    }

    Ok(())
}

pub async fn process_entry(
    account: &Account,
    store: &MailStore,
    path: &Path,
    reject_invalid_certs: bool,
) -> Result<(), VivariumError> {
    if !is_eml(path) || !path.exists() {
        return Ok(());
    }

    let claimed = claim_for_processing(path)?;
    let data = fs::read(&claimed)?;
    policy::authorize_mutation(account, RemoteMutation::Send)?;
    match crate::smtp::send_raw(account, &data, reject_invalid_certs).await {
        Ok(()) => {
            let id = message_id_from_path(path)
                .ok_or_else(|| VivariumError::Message("outbox message has no filename".into()))?;
            store.store_message_in("sent", "cur", &id, &data)?;
            fs::remove_file(&claimed)?;
            tracing::info!(path = %path.display(), "outbox message sent");
            Ok(())
        }
        Err(err) => {
            let failed = failed_path(store, path)?;
            fs::rename(&claimed, &failed)?;
            Err(VivariumError::Smtp(format!(
                "{err}; message moved to {}",
                failed.display()
            )))
        }
    }
}

fn dispatchable_paths(event: &Event) -> Vec<PathBuf> {
    if !matches!(event.kind, EventKind::Create(_) | EventKind::Modify(_)) {
        return Vec::new();
    }
    event
        .paths
        .iter()
        .filter(|path| is_eml(path))
        .cloned()
        .collect()
}

fn claim_for_processing(path: &Path) -> Result<PathBuf, VivariumError> {
    let filename = path
        .file_name()
        .ok_or_else(|| VivariumError::Message("outbox path has no filename".into()))?;
    let tmp = path
        .parent()
        .and_then(|p| p.parent())
        .ok_or_else(|| VivariumError::Message("outbox path has no folder".into()))?
        .join("tmp");
    fs::create_dir_all(&tmp)?;
    let claimed = tmp.join(filename).with_extension("eml.processing");
    fs::rename(path, &claimed)?;
    Ok(claimed)
}

fn failed_path(store: &MailStore, original: &Path) -> Result<PathBuf, VivariumError> {
    let filename = original
        .file_name()
        .ok_or_else(|| VivariumError::Message("outbox path has no filename".into()))?;
    let failed_dir = store.folder_path("outbox").join("failed");
    fs::create_dir_all(&failed_dir)?;
    Ok(failed_dir.join(filename).with_extension("eml.error"))
}

fn is_eml(path: &Path) -> bool {
    path.extension().and_then(|ext| ext.to_str()) == Some("eml")
}

#[cfg(test)]
mod tests {
    use notify::event::{CreateKind, DataChange, ModifyKind};

    use super::*;
    use crate::config::{Auth, MutationPolicy, Provider, Security};

    fn test_account_with_policy(policy: MutationPolicy) -> Account {
        Account {
            name: "test".into(),
            email: "test@example.com".into(),
            imap_host: "localhost".into(),
            imap_port: Some(1143),
            imap_security: Some(Security::Starttls),
            smtp_host: "localhost".into(),
            smtp_port: Some(1025),
            smtp_security: Some(Security::Starttls),
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
            storage_mode: None,
            provider: Provider::Standard,
            oauth_authorization_url: None,
            oauth_token_url: None,
            oauth_scope: None,
            reject_invalid_certs: None,
            policy,
        }
    }

    #[test]
    fn dispatchable_paths_filters_eml_creates_and_modifies() {
        let event = Event {
            kind: EventKind::Create(CreateKind::File),
            paths: vec![PathBuf::from("a.eml"), PathBuf::from("a.txt")],
            attrs: Default::default(),
        };
        assert_eq!(dispatchable_paths(&event), vec![PathBuf::from("a.eml")]);

        let event = Event {
            kind: EventKind::Modify(ModifyKind::Data(DataChange::Content)),
            paths: vec![PathBuf::from("b.eml")],
            attrs: Default::default(),
        };
        assert_eq!(dispatchable_paths(&event), vec![PathBuf::from("b.eml")]);
    }

    #[test]
    fn claim_for_processing_moves_to_tmp() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("outbox/new/message.eml");
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, b"Subject: hello\r\n\r\nbody").unwrap();

        let claimed = claim_for_processing(&path).unwrap();

        assert_eq!(
            claimed,
            tmp.path().join("outbox/tmp/message.eml.processing")
        );
        assert!(!path.exists());
        assert!(claimed.exists());
    }

    #[tokio::test]
    async fn process_entry_denies_send_under_read_only_policy() {
        let tmp = tempfile::tempdir().unwrap();
        let store = MailStore::new(tmp.path());
        let acct = test_account_with_policy(MutationPolicy::ReadOnly);

        // Create a valid .eml in the outbox.
        let outbox_new = tmp.path().join("outbox/new");
        fs::create_dir_all(&outbox_new).unwrap();
        let path = outbox_new.join("msg.eml");
        fs::write(&path, b"Subject: hi\r\n\r\nbody").unwrap();

        // The policy check must fire before any SMTP/network call.
        let err = process_entry(&acct, &store, &path, false)
            .await
            .unwrap_err();
        assert!(err.to_string().contains("policy"));
        assert!(err.to_string().contains("send"));
    }

    #[tokio::test]
    async fn process_entry_denies_send_under_archive_policy() {
        let tmp = tempfile::tempdir().unwrap();
        let store = MailStore::new(tmp.path());
        let acct = test_account_with_policy(MutationPolicy::Archive);

        let outbox_new = tmp.path().join("outbox/new");
        fs::create_dir_all(&outbox_new).unwrap();
        let path = outbox_new.join("msg.eml");
        fs::write(&path, b"Subject: hi\r\n\r\nbody").unwrap();

        let err = process_entry(&acct, &store, &path, false)
            .await
            .unwrap_err();
        assert!(err.to_string().contains("policy"));
    }
}

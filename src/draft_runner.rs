use std::path::{Path, PathBuf};

use vivarium::VivariumError;
use vivarium::catalog::Catalog;
use vivarium::cli::Command;
use vivarium::message::{self, ComposeDraft, ReplyDraft};
use vivarium::store::{MailStore, message_id_from_path};

use super::Runtime;

pub(super) enum DraftDispatch {
    Handled,
    Unhandled(Command),
}

impl Runtime {
    pub(super) async fn run_draft_command(
        &self,
        command: Command,
    ) -> Result<DraftDispatch, VivariumError> {
        match command {
            Command::Send { path } => self.send(&path).await?,
            Command::Reply {
                handle,
                body,
                append_remote,
            } => self.reply(&handle, body, append_remote).await?,
            Command::Compose {
                to,
                cc,
                bcc,
                subject,
                body,
                append_remote,
            } => {
                self.compose(to, cc, bcc, subject, body, append_remote)
                    .await?
            }
            other => return Ok(DraftDispatch::Unhandled(other)),
        }
        Ok(DraftDispatch::Handled)
    }

    async fn send(&self, path: &Path) -> Result<(), VivariumError> {
        require_eml_path(path)?;
        let acct = self.resolve_account(self.account.clone())?;
        let store = MailStore::new(&acct.mail_path(&self.config));
        let data = std::fs::read(path)?;
        let reject_invalid_certs = acct.reject_invalid_certs(&self.config) && !self.insecure;
        vivarium::smtp::send_raw(&acct, &data, reject_invalid_certs).await?;
        let sent = reconcile_sent(&store, path, &data)?;
        println!("sent {}", path.display());
        println!("sent copy: {}", sent.display());
        Ok(())
    }

    async fn compose(
        &self,
        to: Vec<String>,
        cc: Vec<String>,
        bcc: Vec<String>,
        subject: String,
        body: Option<String>,
        append_remote: bool,
    ) -> Result<(), VivariumError> {
        let acct = self.resolve_account(self.account.clone())?;
        let draft = ComposeDraft {
            from: acct.email.clone(),
            to,
            cc,
            bcc,
            subject,
            body: body.unwrap_or_default(),
        };
        let initial = message::build_compose_draft(&draft)?;
        let Some(data) = edit_if_needed("compose", initial, draft.body.is_empty())? else {
            println!("compose cancelled");
            return Ok(());
        };
        let path = store_draft(self, data.as_bytes(), append_remote).await?;
        println!("draft created: {}", path.display());
        Ok(())
    }

    async fn reply(
        &self,
        handle: &str,
        body: Option<String>,
        append_remote: bool,
    ) -> Result<(), VivariumError> {
        let acct = self.resolve_account(self.account.clone())?;
        let store = MailStore::new(&acct.mail_path(&self.config));
        let original = read_by_handle_or_id(&store, &acct.name, handle)?;
        let should_edit = body.is_none();
        let initial = message::build_reply(
            &original,
            &ReplyDraft {
                from: acct.email.clone(),
                body: body.unwrap_or_default(),
            },
        )?;
        let Some(data) = edit_if_needed("reply", initial, should_edit)? else {
            println!("reply cancelled");
            return Ok(());
        };
        let path = store_draft(self, data.as_bytes(), append_remote).await?;
        println!("reply draft created: {}", path.display());
        Ok(())
    }
}

async fn store_draft(
    runtime: &Runtime,
    data: &[u8],
    append_remote: bool,
) -> Result<PathBuf, VivariumError> {
    message::validate_message_headers(data)?;
    let acct = runtime.resolve_account(runtime.account.clone())?;
    let store = MailStore::new(&acct.mail_path(&runtime.config));
    let draft_id = draft_id();
    let path = store.store_message("drafts", &draft_id, data)?;
    if append_remote {
        let reject_invalid_certs = acct.reject_invalid_certs(&runtime.config) && !runtime.insecure;
        vivarium::imap::append_message(&acct, &acct.drafts_folder(), data, reject_invalid_certs)
            .await?;
    }
    Ok(path)
}

fn reconcile_sent(
    store: &MailStore,
    source_path: &Path,
    data: &[u8],
) -> Result<PathBuf, VivariumError> {
    let message_id = message_id_from_path(source_path).unwrap_or_else(draft_id);
    let remove_draft = source_is_local_draft(store, source_path, &message_id);
    let sent_path = store.store_message_in("sent", "cur", &message_id, data)?;
    if remove_draft {
        store.remove_message(&message_id, "drafts")?;
    }
    Ok(sent_path)
}

fn source_is_local_draft(store: &MailStore, source_path: &Path, message_id: &str) -> bool {
    let Ok(location) = store.locate_message(message_id) else {
        return false;
    };
    location.folder == "Drafts" && same_file(&location.path, source_path)
}

pub(crate) fn read_by_handle_or_id(
    store: &MailStore,
    account: &str,
    handle: &str,
) -> Result<Vec<u8>, VivariumError> {
    if let Ok(data) = store.read_message(handle) {
        return Ok(data);
    }
    let catalog = Catalog::open(store.root())?;
    let entry = catalog
        .resolve_entry(account, handle)
        .ok_or_else(|| VivariumError::Message(format!("message not found for reply: {handle}")))?;
    let id = message_id_from_path(Path::new(&entry.raw_path)).ok_or_else(|| {
        VivariumError::Message(format!("catalog entry has no local message id: {handle}"))
    })?;
    store.read_message(&id)
}

fn edit_if_needed(
    prefix: &str,
    initial: String,
    should_edit: bool,
) -> Result<Option<String>, VivariumError> {
    if !should_edit {
        return Ok(Some(initial));
    }
    let Some(edited) = edit_message(prefix, initial.as_bytes())? else {
        return Ok(None);
    };
    String::from_utf8(edited)
        .map(Some)
        .map_err(|e| VivariumError::Message(format!("edited draft is not UTF-8: {e}")))
}

fn edit_message(prefix: &str, initial: &[u8]) -> Result<Option<Vec<u8>>, VivariumError> {
    let path = editor_temp_path(prefix);
    std::fs::write(&path, initial)?;
    let editor = std::env::var("VISUAL")
        .or_else(|_| std::env::var("EDITOR"))
        .unwrap_or_else(|_| "vi".to_string());
    let status = std::process::Command::new("sh")
        .arg("-c")
        .arg(format!("{} \"$1\"", editor))
        .arg("vivarium-editor")
        .arg(&path)
        .status()?;
    if !status.success() {
        std::fs::remove_file(&path).ok();
        return Ok(None);
    }
    let edited = std::fs::read(&path)?;
    std::fs::remove_file(&path).ok();
    Ok(Some(edited))
}

fn editor_temp_path(prefix: &str) -> PathBuf {
    let unique = format!(
        "vivarium-{prefix}-{}-{}.eml",
        std::process::id(),
        chrono::Utc::now().timestamp_nanos_opt().unwrap_or_default()
    );
    std::env::temp_dir().join(Path::new(&unique))
}

pub(crate) fn require_eml_path(path: &Path) -> Result<(), VivariumError> {
    if path.extension().and_then(|ext| ext.to_str()) == Some("eml") {
        Ok(())
    } else {
        Err(VivariumError::Message(format!(
            "send requires an explicit .eml file: {}",
            path.display()
        )))
    }
}

fn same_file(left: &Path, right: &Path) -> bool {
    let left = std::fs::canonicalize(left).ok();
    let right = std::fs::canonicalize(right).ok();
    left.is_some() && left == right
}

fn draft_id() -> String {
    format!(
        "draft-{}",
        chrono::Utc::now().timestamp_nanos_opt().unwrap_or_default()
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sent_reconciliation_moves_local_draft_to_sent() {
        let tmp = tempfile::tempdir().unwrap();
        let store = MailStore::new(tmp.path());
        let data = b"From: me@example.com\r\nTo: you@example.com\r\nSubject: hi\r\n\r\nbody";
        let draft = store.store_message("drafts", "draft-1", data).unwrap();

        let sent = reconcile_sent(&store, &draft, data).unwrap();

        assert!(sent.ends_with("Sent/cur/draft-1.eml:2,S"));
        assert!(sent.exists());
        assert!(!draft.exists());
    }

    #[test]
    fn require_eml_path_rejects_non_eml() {
        let err = require_eml_path(Path::new("message.txt")).unwrap_err();

        assert!(err.to_string().contains(".eml"));
    }
}

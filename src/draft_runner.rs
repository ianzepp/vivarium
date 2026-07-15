use std::path::{Path, PathBuf};

use vivarium::VivariumError;
use vivarium::cli::{Command, ComposeCommand};
use vivarium::config::Provider;
use vivarium::message::{self, ComposeDraft, ReplyDraft};
use vivarium::policy::{self, RemoteMutation};
use vivarium::store::{MailStore, message_id_from_path};

use super::Runtime;

pub(super) enum DraftDispatch {
    Handled,
    Unhandled(Box<Command>),
}

impl Runtime {
    pub(super) async fn run_draft_command(
        &self,
        command: Command,
    ) -> Result<DraftDispatch, VivariumError> {
        match command {
            Command::Reply(vivarium::cli::ReplyCommand {
                handle,
                from,
                body,
                html_body,
                html_body_auto,
                append_remote,
            }) => {
                self.reply(
                    &handle,
                    from,
                    body,
                    html_body,
                    html_body_auto,
                    append_remote,
                )
                .await?
            }
            Command::Compose(command) => self.compose(command).await?,
            other => return Ok(DraftDispatch::Unhandled(Box::new(other))),
        }
        Ok(DraftDispatch::Handled)
    }

    pub(super) async fn send_path(
        &self,
        path: &Path,
        from: Option<&str>,
    ) -> Result<(), VivariumError> {
        require_eml_path(path)?;
        let acct = self.resolve_account(self.account.clone())?;
        let store = MailStore::new(&acct.mail_path(&self.config));
        let mut data = std::fs::read(path)?;
        if let Some(from) = from {
            data = message::replace_from_header(&data, from)?;
        }
        if send_transport(&acct.provider) == SendTransport::DirectProtonApi {
            vivarium::proton_send::send_raw(&acct, &self.config, &data).await?;
        } else {
            let reject_invalid_certs = acct.reject_invalid_certs(&self.config) && !self.insecure;
            vivarium::smtp::send_raw(&acct, &data, reject_invalid_certs).await?;
        }
        let sent = reconcile_sent(&store, path, &data)?;
        println!("sent {}", path.display());
        println!("sent copy: {}", sent.display());
        Ok(())
    }

    async fn compose(&self, command: ComposeCommand) -> Result<(), VivariumError> {
        let ComposeCommand {
            to,
            from,
            cc,
            bcc,
            subject,
            body,
            html_body,
            html_body_auto,
            append_remote,
        } = command;
        let acct = self.resolve_account(self.account.clone())?;
        let body = body.unwrap_or_default();
        let draft = ComposeDraft {
            from: from.unwrap_or_else(|| acct.email.clone()),
            to,
            cc,
            bcc,
            subject,
            html_body: resolve_html_body(&body, html_body, html_body_auto),
            body,
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
        from: Option<String>,
        body: Option<String>,
        html_body: Option<String>,
        html_body_auto: bool,
        append_remote: bool,
    ) -> Result<(), VivariumError> {
        let acct = self.resolve_account(self.account.clone())?;
        let store = MailStore::new(&acct.mail_path(&self.config));
        let original = read_by_handle_or_id(&store, &acct.name, handle)?;
        let should_edit = body.is_none();
        let body = body.unwrap_or_default();
        let initial = message::build_reply(
            &original,
            &ReplyDraft {
                from: from.unwrap_or_else(|| acct.email.clone()),
                html_body: resolve_html_body(&body, html_body, html_body_auto),
                body,
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

    pub(super) async fn reply_body(&self, handle: &str, body: String) -> Result<(), VivariumError> {
        self.reply(handle, None, Some(body), None, false, false)
            .await
    }
}

#[derive(Debug, Eq, PartialEq)]
enum SendTransport {
    DirectProtonApi,
    Smtp,
}

fn send_transport(provider: &Provider) -> SendTransport {
    match provider {
        Provider::ProtonApi => SendTransport::DirectProtonApi,
        Provider::Protonmail | Provider::Gmail | Provider::Standard => SendTransport::Smtp,
    }
}

fn resolve_html_body(
    body: &str,
    html_body: Option<String>,
    html_body_auto: bool,
) -> Option<String> {
    if html_body_auto {
        Some(message::auto_html_body(body))
    } else {
        html_body
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
        policy::authorize_mutation(&acct, RemoteMutation::AppendDraft)?;
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
    let Ok(canonical_source) = source_path.canonicalize() else {
        return false;
    };
    let Ok(canonical_drafts) = store.folder_path("drafts").canonicalize() else {
        return false;
    };
    if !canonical_source.starts_with(canonical_drafts) {
        return false;
    }
    message_id_from_path(source_path).as_deref() == Some(message_id)
}

pub(crate) fn read_by_handle_or_id(
    store: &MailStore,
    _account: &str,
    handle: &str,
) -> Result<Vec<u8>, VivariumError> {
    store
        .read_message(handle)
        .map_err(|_| VivariumError::Message(format!("message not found for reply: {handle}")))
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

    #[test]
    fn send_transport_routes_only_direct_proton_api_away_from_smtp() {
        assert_eq!(
            send_transport(&Provider::ProtonApi),
            SendTransport::DirectProtonApi
        );
        assert_eq!(send_transport(&Provider::Protonmail), SendTransport::Smtp);
        assert_eq!(send_transport(&Provider::Gmail), SendTransport::Smtp);
        assert_eq!(send_transport(&Provider::Standard), SendTransport::Smtp);
    }
}

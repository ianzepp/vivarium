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
            attachments,
            attach_document,
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
        let attachments = vivarium::render::compose_attachments(
            &attachments,
            attach_document.as_deref(),
            &self.config,
            None,
        )?;
        let initial = message::build_compose_draft_with_attachments(&draft, &attachments)?;
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
    // Store sent copy first; source draft is only removed after success.
    let sent_path = store.store_message_in("sent", "cur", &message_id, data)?;
    if remove_draft {
        // If draft removal fails, the send still succeeded and the sent copy
        // is durable. Return an explicit recovery error rather than hiding it.
        store.remove_message(&message_id, "drafts").map_err(|e| {
            VivariumError::Other(format!(
                "send succeeded and sent copy stored at {}, but failed to remove source draft: {e}",
                sent_path.display()
            ))
        })?;
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
    fn sent_reconciliation_preserves_non_draft_source() {
        let tmp = tempfile::tempdir().unwrap();
        let store = MailStore::new(tmp.path());
        let data = b"From: me@example.com\r\nTo: you@example.com\r\nSubject: hi\r\n\r\nbody";
        // Source from outside drafts (e.g. /tmp/draft.eml) — must not be removed.
        let external = tmp.path().join("external.eml");
        std::fs::write(&external, data).unwrap();

        let sent = reconcile_sent(&store, &external, data).unwrap();

        assert!(sent.exists());
        assert!(external.exists(), "non-draft source must be preserved");
    }

    #[test]
    fn sent_reconciliation_durable_sent_copy_before_draft_removal() {
        let tmp = tempfile::tempdir().unwrap();
        let store = MailStore::new(tmp.path());
        let data = b"From: me@example.com\r\nTo: you@example.com\r\nSubject: hi\r\n\r\nbody";
        let draft = store.store_message("drafts", "draft-2", data).unwrap();

        let sent = reconcile_sent(&store, &draft, data).unwrap();

        // Sent copy must exist before draft was removed (ordering invariant).
        assert!(sent.exists(), "sent copy must be durable");
        assert!(!draft.exists(), "draft removed only after sent copy stored");
    }

    #[test]
    fn sent_reconciliation_failure_preserves_source() {
        let tmp = tempfile::tempdir().unwrap();
        let store = MailStore::new(tmp.path());
        let data = b"From: me@example.com\r\nTo: you@example.com\r\nSubject: hi\r\n\r\nbody";
        let draft = store.store_message("drafts", "draft-3", data).unwrap();

        // Block Sent directory creation by placing a file where Sent/cur/file
        // would go. store_message_in creates parent dirs, so we block the
        // final write by making the file path occupied with a directory.
        let sent_dir = tmp.path().join("Sent");
        let blocker = sent_dir.join("cur");
        std::fs::create_dir_all(&sent_dir).unwrap();
        std::fs::write(&blocker, b"blocker").unwrap();

        let result = reconcile_sent(&store, &draft, data);

        assert!(result.is_err(), "must fail when sent copy cannot persist");
        assert!(draft.exists(), "source must survive sent-copy failure");
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

    fn test_runtime(
        tmp: &std::path::Path,
        policy: vivarium::config::MutationPolicy,
    ) -> super::Runtime {
        use vivarium::config::{Account, AccountsFile, Auth, Config, Security};

        let account = Account {
            name: "test".into(),
            email: "test@example.com".into(),
            imap_host: "localhost".into(),
            imap_port: Some(1143),
            imap_security: Some(Security::Starttls),
            smtp_host: "localhost".into(),
            smtp_port: Some(1025),
            smtp_security: Some(Security::Starttls),
            username: "test".into(),
            auth: Auth::Password,
            password: Some("secret".into()),
            password_cmd: None,
            token_cmd: None,
            oauth_client_id: None,
            oauth_client_secret: None,
            mail_dir: Some(tmp.to_string_lossy().to_string()),
            inbox_folder: None,
            archive_folder: None,
            trash_folder: None,
            sent_folder: None,
            drafts_folder: None,
            label_roots: None,
            storage_mode: None,
            provider: vivarium::config::Provider::Standard,
            oauth_authorization_url: None,
            oauth_token_url: None,
            oauth_scope: None,
            reject_invalid_certs: None,
            policy,
        };
        super::Runtime {
            config: Config::default(),
            accounts: AccountsFile {
                accounts: vec![account],
            },
            account: Some("test".into()),
            insecure: false,
        }
    }

    #[tokio::test]
    async fn store_draft_append_remote_denied_under_read_only() {
        let tmp = tempfile::tempdir().unwrap();
        let runtime = test_runtime(tmp.path(), vivarium::config::MutationPolicy::ReadOnly);
        let data = b"From: test@example.com\r\nTo: you@example.com\r\nSubject: hi\r\n\r\nbody";

        let err = store_draft(&runtime, data, true).await.unwrap_err();
        assert!(matches!(err, VivariumError::Policy(_)));
        assert!(err.to_string().contains("append-draft"));
    }

    #[tokio::test]
    async fn store_draft_append_remote_denied_under_archive() {
        let tmp = tempfile::tempdir().unwrap();
        let runtime = test_runtime(tmp.path(), vivarium::config::MutationPolicy::Archive);
        let data = b"From: test@example.com\r\nTo: you@example.com\r\nSubject: hi\r\n\r\nbody";

        let err = store_draft(&runtime, data, true).await.unwrap_err();
        assert!(matches!(err, VivariumError::Policy(_)));
    }

    #[tokio::test]
    async fn store_draft_append_remote_allowed_under_full_write() {
        let tmp = tempfile::tempdir().unwrap();
        let runtime = test_runtime(tmp.path(), vivarium::config::MutationPolicy::FullWrite);
        let data = b"From: test@example.com\r\nTo: you@example.com\r\nSubject: hi\r\n\r\nbody";

        // Local draft is created; append_remote gate passes under full-write.
        // The actual IMAP APPEND will fail because no server is running,
        // but the error must NOT be Policy — proving authorization passed.
        let err = store_draft(&runtime, data, true).await.unwrap_err();
        assert!(!matches!(err, VivariumError::Policy(_)));

        // The local draft must be stored before the remote append is attempted,
        // preserving the local-first invariant even when the remote fails.
        let mail_root = runtime
            .resolve_account(runtime.account.clone())
            .unwrap()
            .mail_path(&runtime.config);
        let drafts_dir = mail_root.join("Drafts");
        assert!(drafts_dir.exists(), "local drafts directory must exist");
    }

    #[tokio::test]
    async fn store_draft_local_only_under_read_only() {
        let tmp = tempfile::tempdir().unwrap();
        let runtime = test_runtime(tmp.path(), vivarium::config::MutationPolicy::ReadOnly);
        let data = b"From: test@example.com\r\nTo: you@example.com\r\nSubject: hi\r\n\r\nbody";

        // Local-only draft (append_remote=false) must succeed under any policy.
        let path = store_draft(&runtime, data, false).await.unwrap();
        assert!(path.exists());
    }
}

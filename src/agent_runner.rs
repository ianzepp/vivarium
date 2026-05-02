use std::path::PathBuf;

use vivarium::VivariumError;
use vivarium::agent::{self, DEFAULT_MAX_BODY_BYTES};
use vivarium::cli::{AgentCommand, Command};
use vivarium::message::{self, ReplyDraft};
use vivarium::store::MailStore;

use super::Runtime;
use crate::draft_runner::read_by_handle_or_id;

pub(super) enum AgentDispatch {
    Handled,
    Unhandled(Command),
}

impl Runtime {
    pub(super) async fn run_agent_command(
        &self,
        command: Command,
    ) -> Result<AgentDispatch, VivariumError> {
        let Command::Agent { command } = command else {
            return Ok(AgentDispatch::Unhandled(command));
        };
        match command {
            AgentCommand::Archive { handle, execute } => {
                self.agent_archive(handle, execute).await?
            }
            AgentCommand::Delete {
                handle,
                expunge,
                confirm,
                execute,
            } => self.agent_delete(handle, expunge, confirm, execute).await?,
            AgentCommand::Move {
                handle,
                folder,
                execute,
            } => self.agent_move(handle, folder, execute).await?,
            AgentCommand::Flag {
                handle,
                read,
                unread,
                star,
                unstar,
                execute,
            } => {
                self.agent_flag(handle, read, unread, star, unstar, execute)
                    .await?
            }
            AgentCommand::Send { path, execute } => self.agent_send(path, execute).await?,
            AgentCommand::Reply {
                handle,
                body,
                execute,
            } => self.agent_reply(handle, body, execute).await?,
        }
        Ok(AgentDispatch::Handled)
    }

    async fn agent_archive(&self, handle: String, execute: bool) -> Result<(), VivariumError> {
        self.run_mutation_command(Command::Archive {
            handles: vec![handle],
            dry_run: !execute,
            json: true,
        })
        .await?;
        Ok(())
    }

    async fn agent_delete(
        &self,
        handle: String,
        expunge: bool,
        confirm: bool,
        execute: bool,
    ) -> Result<(), VivariumError> {
        if expunge && !self.config.defaults.agent_allow_hard_delete {
            return Err(VivariumError::Message(
                "agent hard delete is disabled by config default".into(),
            ));
        }
        self.run_mutation_command(Command::Delete {
            handle,
            trash: !expunge,
            expunge,
            confirm,
            dry_run: !execute,
            json: true,
        })
        .await?;
        Ok(())
    }

    async fn agent_move(
        &self,
        handle: String,
        folder: String,
        execute: bool,
    ) -> Result<(), VivariumError> {
        self.run_mutation_command(Command::Move {
            handle,
            folder,
            dry_run: !execute,
            json: true,
        })
        .await?;
        Ok(())
    }

    async fn agent_flag(
        &self,
        handle: String,
        read: bool,
        unread: bool,
        star: bool,
        unstar: bool,
        execute: bool,
    ) -> Result<(), VivariumError> {
        self.run_mutation_command(Command::Flag {
            handle,
            read,
            unread,
            star,
            unstar,
            dry_run: !execute,
            json: true,
        })
        .await?;
        Ok(())
    }

    async fn agent_send(&self, path: PathBuf, execute: bool) -> Result<(), VivariumError> {
        crate::draft_runner::require_eml_path(&path)?;
        let acct = self.resolve_account(self.account.clone())?;
        let mail_root = acct.mail_path(&self.config);
        let target = path.to_string_lossy().to_string();
        audit(
            &mail_root,
            agent::audit_record(&acct.name, "send", "planned", &target, true, false, None),
        )?;
        let preview = serde_json::json!({ "path": target });
        if !execute {
            return print_agent_plan(&acct.name, "send", &target, true, false, preview);
        }
        audit(
            &mail_root,
            agent::audit_record(&acct.name, "send", "approved", &target, true, true, None),
        )?;
        let result = self.run_draft_command(Command::Send { path }).await;
        finish_outbound(&mail_root, &acct.name, "send", &target, true, result)?;
        print_agent_plan(&acct.name, "send", &target, true, true, preview)
    }

    async fn agent_reply(
        &self,
        handle: String,
        body: String,
        execute: bool,
    ) -> Result<(), VivariumError> {
        let acct = self.resolve_account(self.account.clone())?;
        let store = MailStore::new(&acct.mail_path(&self.config));
        let original = read_by_handle_or_id(&store, &acct.name, &handle)?;
        let draft = message::build_reply(
            &original,
            &ReplyDraft {
                from: acct.email.clone(),
                body: body.clone(),
            },
        )?;
        let preview = reply_preview(&draft, self.agent_max_body_bytes());
        let mail_root = acct.mail_path(&self.config);
        audit(
            &mail_root,
            agent::audit_record(&acct.name, "reply", "planned", &handle, false, false, None),
        )?;
        if !execute {
            return print_agent_plan(&acct.name, "reply", &handle, false, false, preview);
        }
        audit(
            &mail_root,
            agent::audit_record(&acct.name, "reply", "approved", &handle, false, true, None),
        )?;
        let result = self
            .run_draft_command(Command::Reply {
                handle: handle.clone(),
                body: Some(body),
                append_remote: false,
            })
            .await;
        finish_outbound(&mail_root, &acct.name, "reply", &handle, false, result)?;
        print_agent_plan(&acct.name, "reply", &handle, false, true, preview)
    }

    fn agent_max_body_bytes(&self) -> usize {
        self.config
            .defaults
            .agent_max_body_bytes
            .unwrap_or(DEFAULT_MAX_BODY_BYTES)
    }
}

fn finish_outbound(
    mail_root: &std::path::Path,
    account: &str,
    operation: &str,
    target: &str,
    external_write: bool,
    result: Result<super::draft_runner::DraftDispatch, VivariumError>,
) -> Result<(), VivariumError> {
    match result {
        Ok(_) => {
            audit(
                mail_root,
                agent::audit_record(
                    account,
                    operation,
                    "executed",
                    target,
                    external_write,
                    true,
                    None,
                ),
            )?;
            audit(
                mail_root,
                agent::audit_record(
                    account,
                    operation,
                    "reconciled",
                    target,
                    external_write,
                    true,
                    None,
                ),
            )
        }
        Err(err) => {
            audit(
                mail_root,
                agent::audit_record(
                    account,
                    operation,
                    "failed",
                    target,
                    external_write,
                    true,
                    Some(err.to_string()),
                ),
            )?;
            Err(err)
        }
    }
}

fn reply_preview(draft: &str, max_bytes: usize) -> serde_json::Value {
    let bounded = agent::bounded_text(draft, max_bytes);
    serde_json::json!({
        "draft": bounded,
        "append_remote": false,
    })
}

fn print_agent_plan(
    account: &str,
    operation: &str,
    target: &str,
    external_write: bool,
    execute: bool,
    preview: serde_json::Value,
) -> Result<(), VivariumError> {
    let json = agent::plan_json(account, operation, target, external_write, execute, preview);
    println!("{}", serde_json::to_string_pretty(&json).unwrap());
    Ok(())
}

fn audit(
    mail_root: &std::path::Path,
    record: vivarium::agent::AgentAuditRecord,
) -> Result<(), VivariumError> {
    agent::append_audit(mail_root, record)?;
    Ok(())
}

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
            AgentCommand::Archive { handles } => self.agent_archive(handles).await?,
            AgentCommand::Delete { handles, expunge } => {
                self.agent_delete(handles, expunge).await?
            }
            AgentCommand::Move { handle, folder } => self.agent_move(handle, folder).await?,
            AgentCommand::Flag {
                handle,
                read,
                unread,
                star,
                unstar,
            } => self.agent_flag(handle, read, unread, star, unstar).await?,
            AgentCommand::Send { path } => self.agent_send(path).await?,
            AgentCommand::Reply { handle, body } => self.agent_reply(handle, body).await?,
        }
        Ok(AgentDispatch::Handled)
    }

    async fn agent_archive(&self, handles: Vec<String>) -> Result<(), VivariumError> {
        self.run_mutation_command(Command::Archive {
            handles: unique_handles(handles),
            dry_run: true,
            json: true,
        })
        .await?;
        Ok(())
    }

    async fn agent_delete(&self, handles: Vec<String>, expunge: bool) -> Result<(), VivariumError> {
        self.run_mutation_command(Command::Delete {
            handles: unique_handles(handles),
            trash: !expunge,
            expunge,
            confirm: false,
            dry_run: true,
            json: true,
        })
        .await?;
        Ok(())
    }

    async fn agent_move(&self, handle: String, folder: String) -> Result<(), VivariumError> {
        self.run_mutation_command(Command::Move {
            handle,
            folder,
            dry_run: true,
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
    ) -> Result<(), VivariumError> {
        self.run_mutation_command(Command::Flag {
            handle,
            read,
            unread,
            star,
            unstar,
            dry_run: true,
            json: true,
        })
        .await?;
        Ok(())
    }

    async fn agent_send(&self, path: PathBuf) -> Result<(), VivariumError> {
        crate::draft_runner::require_eml_path(&path)?;
        let acct = self.resolve_account(self.account.clone())?;
        let mail_root = acct.mail_path(&self.config);
        let target = path.to_string_lossy().to_string();
        audit(
            &mail_root,
            agent::audit_record(&acct.name, "send", "planned", &target, true, false, None),
        )?;
        let preview = serde_json::json!({ "path": target });
        print_agent_plan(&acct.name, "send", &target, true, preview)
    }

    async fn agent_reply(&self, handle: String, body: String) -> Result<(), VivariumError> {
        let acct = self.resolve_account(self.account.clone())?;
        let store = MailStore::new(&acct.mail_path(&self.config));
        let original = read_by_handle_or_id(&store, &acct.name, &handle)?;
        let draft = message::build_reply(
            &original,
            &ReplyDraft {
                from: acct.email.clone(),
                body: body.clone(),
                html_body: None,
            },
        )?;
        let preview = reply_preview(&draft, self.agent_max_body_bytes());
        let mail_root = acct.mail_path(&self.config);
        audit(
            &mail_root,
            agent::audit_record(&acct.name, "reply", "planned", &handle, false, false, None),
        )?;
        print_agent_plan(&acct.name, "reply", &handle, false, preview)
    }

    fn agent_max_body_bytes(&self) -> usize {
        self.config
            .defaults
            .agent_max_body_bytes
            .unwrap_or(DEFAULT_MAX_BODY_BYTES)
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
    preview: serde_json::Value,
) -> Result<(), VivariumError> {
    let json = agent::plan_json(account, operation, target, external_write, preview);
    println!("{}", serde_json::to_string_pretty(&json).unwrap());
    Ok(())
}

fn unique_handles(handles: Vec<String>) -> Vec<String> {
    let mut unique = Vec::with_capacity(handles.len());
    for handle in handles {
        if !unique.contains(&handle) {
            unique.push(handle);
        }
    }
    unique
}

fn audit(
    mail_root: &std::path::Path,
    record: vivarium::agent::AgentAuditRecord,
) -> Result<(), VivariumError> {
    agent::append_audit(mail_root, record)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unique_handles_preserves_first_seen_order() {
        let handles = unique_handles(vec![
            "one".into(),
            "two".into(),
            "one".into(),
            "three".into(),
            "two".into(),
        ]);

        assert_eq!(handles, vec!["one", "two", "three"]);
    }
}

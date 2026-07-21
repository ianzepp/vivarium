use vivarium::VivariumError;
use vivarium::cli::{AgentCommand, Command, EnqueueCommand, ExecCommand, QueueCommand};
use vivarium::policy;
use vivarium::queue::{self, QueueItem, QueueStatus, QueuedCommand};

use super::Runtime;
use crate::draft_runner::require_eml_path;

pub(super) enum QueueDispatch {
    Handled,
    Unhandled(Box<Command>),
}

impl Runtime {
    pub(super) async fn run_queue_command(
        &self,
        command: Command,
    ) -> Result<QueueDispatch, VivariumError> {
        match command {
            Command::Exec { command } => self.exec(command).await?,
            Command::Enqueue { command } => self.enqueue(command)?,
            Command::Queue { command } => self.queue(command).await?,
            Command::Agent { command: agent } if is_agent_mutation(&agent) => {
                self.run_agent_mutation_command(agent).await?;
            }
            other => return Ok(QueueDispatch::Unhandled(Box::new(other))),
        }
        Ok(QueueDispatch::Handled)
    }

    async fn run_agent_mutation_command(&self, command: AgentCommand) -> Result<(), VivariumError> {
        let (queued, json, execute) = queued_from_agent(command)?;
        if execute {
            self.execute_queued(queued, json).await
        } else {
            self.plan_queued(queued, json).await
        }
    }

    async fn exec(&self, command: ExecCommand) -> Result<(), VivariumError> {
        let (command, json) = queued_from_exec(command)?;
        self.execute_queued(command, json).await
    }

    fn enqueue(&self, command: EnqueueCommand) -> Result<(), VivariumError> {
        let acct = self.resolve_account(self.account.clone())?;
        let queued = queued_from_enqueue(command)?;
        policy::authorize(&acct, &queued)?;
        let item = QueueItem::new(acct.name.clone(), queued);
        let path = queue::enqueue(&acct.mail_path(&self.config), &item)?;
        println!("queued {} {}", item.id, path.display());
        Ok(())
    }

    async fn queue(&self, command: QueueCommand) -> Result<(), VivariumError> {
        match command {
            QueueCommand::List { all, json } => self.queue_list(all, json),
            QueueCommand::Show { id, json } => self.queue_show(&id, json),
            QueueCommand::Drop { id } => self.queue_drop(&id),
            QueueCommand::Run { ids, all } => self.queue_run(ids, all).await,
        }
    }

    fn queue_list(&self, all: bool, json: bool) -> Result<(), VivariumError> {
        let acct = self.resolve_account(self.account.clone())?;
        let items = queue::list(&acct.mail_path(&self.config), all)?;
        if json {
            println!("{}", serde_json::to_string_pretty(&items).unwrap());
            return Ok(());
        }
        if items.is_empty() {
            println!("queue empty");
            return Ok(());
        }
        for item in items {
            println!(
                "{} {:?} {:?} {}",
                item.id, item.status, item.command, item.created_at
            );
        }
        Ok(())
    }

    fn queue_show(&self, id: &str, json: bool) -> Result<(), VivariumError> {
        let acct = self.resolve_account(self.account.clone())?;
        let item = queue::load(&acct.mail_path(&self.config), id)?;
        if json {
            println!("{}", serde_json::to_string_pretty(&item).unwrap());
        } else {
            println!("id: {}", item.id);
            println!("status: {:?}", item.status);
            println!("account: {}", item.account);
            println!("created_at: {}", item.created_at);
            println!("command: {:?}", item.command);
            if let Some(error) = item.error {
                println!("error: {error}");
            }
        }
        Ok(())
    }

    fn queue_drop(&self, id: &str) -> Result<(), VivariumError> {
        let acct = self.resolve_account(self.account.clone())?;
        let mail_root = acct.mail_path(&self.config);
        let mut item = queue::load(&mail_root, id)?;
        item.mark(QueueStatus::Dropped, None);
        queue::save(&mail_root, &item)?;
        println!("dropped {}", item.id);
        Ok(())
    }

    async fn queue_run(&self, ids: Vec<String>, all: bool) -> Result<(), VivariumError> {
        if all && !ids.is_empty() {
            return Err(VivariumError::Message(
                "use queue run --all or queue run <id>..., not both".into(),
            ));
        }
        let acct = self.resolve_account(self.account.clone())?;
        let mail_root = acct.mail_path(&self.config);
        let ids = if all {
            queue::pending_ids(&mail_root)?
        } else if ids.is_empty() {
            return Err(VivariumError::Message(
                "queue run needs at least one id, or --all".into(),
            ));
        } else {
            ids
        };
        for id in ids {
            let mut item = queue::load(&mail_root, &id)?;
            if item.status != QueueStatus::Pending {
                return Err(VivariumError::Message(format!(
                    "queued item {} is {:?}, not pending",
                    item.id, item.status
                )));
            }
            match self.execute_queued(item.command.clone(), false).await {
                Ok(()) => {
                    item.mark(QueueStatus::Executed, None);
                    queue::save(&mail_root, &item)?;
                    println!("queue executed {}", item.id);
                }
                Err(err) => {
                    item.mark(QueueStatus::Failed, Some(err.to_string()));
                    queue::save(&mail_root, &item)?;
                    return Err(err);
                }
            }
        }
        Ok(())
    }

    async fn execute_queued(
        &self,
        command: QueuedCommand,
        json: bool,
    ) -> Result<(), VivariumError> {
        let acct = self.resolve_account(self.account.clone())?;
        policy::authorize(&acct, &command)?;
        match command {
            QueuedCommand::Archive { .. }
            | QueuedCommand::Delete { .. }
            | QueuedCommand::Move { .. }
            | QueuedCommand::Flag { .. } => self.run_queued_mutation(command, json, false).await,
            QueuedCommand::Send { path, from } => self.send_path(&path, from.as_deref()).await,
            QueuedCommand::Reply { handle, body } => self.reply_body(&handle, body).await,
        }
    }

    async fn plan_queued(&self, command: QueuedCommand, json: bool) -> Result<(), VivariumError> {
        let acct = self.resolve_account(self.account.clone())?;
        policy::authorize(&acct, &command)?;
        match command {
            QueuedCommand::Archive { .. }
            | QueuedCommand::Delete { .. }
            | QueuedCommand::Move { .. }
            | QueuedCommand::Flag { .. } => self.run_queued_mutation(command, json, true).await,
            QueuedCommand::Send { .. } | QueuedCommand::Reply { .. } => {
                Err(VivariumError::Message("agent plan not supported".into()))
            }
        }
    }
}

fn is_agent_mutation(command: &AgentCommand) -> bool {
    matches!(
        command,
        AgentCommand::Archive { .. }
            | AgentCommand::Delete { .. }
            | AgentCommand::Move { .. }
            | AgentCommand::Flag { .. }
    )
}

fn queued_from_agent(command: AgentCommand) -> Result<(QueuedCommand, bool, bool), VivariumError> {
    match command {
        AgentCommand::Archive {
            handles,
            execute,
            json,
        } => Ok((QueuedCommand::Archive { handles }, json, execute)),
        AgentCommand::Delete {
            handles,
            trash: _,
            expunge,
            confirm,
            execute,
            json,
        } => Ok((
            QueuedCommand::Delete {
                handles,
                expunge,
                confirm,
            },
            json,
            execute,
        )),
        AgentCommand::Move {
            handle,
            folder,
            execute,
            json,
        } => Ok((QueuedCommand::Move { handle, folder }, json, execute)),
        AgentCommand::Flag {
            handle,
            read,
            unread,
            star,
            unstar,
            execute,
            json,
        } => Ok((
            QueuedCommand::Flag {
                handle,
                read,
                unread,
                star,
                unstar,
            },
            json,
            execute,
        )),
        AgentCommand::Poll { .. } => Err(VivariumError::Message(
            "agent poll is not a mutation command".into(),
        )),
    }
}

fn queued_from_exec(command: ExecCommand) -> Result<(QueuedCommand, bool), VivariumError> {
    match command {
        ExecCommand::Archive { handles, json } => Ok((QueuedCommand::Archive { handles }, json)),
        ExecCommand::Delete {
            handles,
            trash: _,
            expunge,
            confirm,
            json,
        } => Ok((
            QueuedCommand::Delete {
                handles,
                expunge,
                confirm,
            },
            json,
        )),
        ExecCommand::Move {
            handle,
            folder,
            json,
        } => Ok((QueuedCommand::Move { handle, folder }, json)),
        ExecCommand::Flag {
            handle,
            read,
            unread,
            star,
            unstar,
            json,
        } => Ok((
            QueuedCommand::Flag {
                handle,
                read,
                unread,
                star,
                unstar,
            },
            json,
        )),
        ExecCommand::Send { path, from } => {
            require_eml_path(&path)?;
            Ok((QueuedCommand::Send { path, from }, false))
        }
    }
}

fn queued_from_enqueue(command: EnqueueCommand) -> Result<QueuedCommand, VivariumError> {
    match command {
        EnqueueCommand::Archive { handles } => Ok(QueuedCommand::Archive { handles }),
        EnqueueCommand::Delete {
            handles,
            trash: _,
            expunge,
            confirm,
        } => Ok(QueuedCommand::Delete {
            handles,
            expunge,
            confirm,
        }),
        EnqueueCommand::Move { handle, folder } => Ok(QueuedCommand::Move { handle, folder }),
        EnqueueCommand::Flag {
            handle,
            read,
            unread,
            star,
            unstar,
        } => Ok(QueuedCommand::Flag {
            handle,
            read,
            unread,
            star,
            unstar,
        }),
        EnqueueCommand::Send { path, from } => {
            require_eml_path(&path)?;
            Ok(QueuedCommand::Send { path, from })
        }
        EnqueueCommand::Reply { handle, body } => Ok(QueuedCommand::Reply { handle, body }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use vivarium::config::{Account, AccountsFile, Auth, Config, MutationPolicy, Security};

    fn test_runtime(tmp: &std::path::Path, policy: MutationPolicy) -> Runtime {
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
        Runtime {
            config: Config::default(),
            accounts: AccountsFile {
                accounts: vec![account],
            },
            account: Some("test".into()),
            insecure: false,
        }
    }

    #[tokio::test]
    async fn queue_run_rejects_stale_delete_under_read_only() {
        let tmp = tempfile::tempdir().unwrap();
        let runtime = test_runtime(tmp.path(), MutationPolicy::ReadOnly);

        let stale = QueueItem::new(
            "test".into(),
            QueuedCommand::Delete {
                handles: vec!["old".into()],
                expunge: false,
                confirm: true,
            },
        );
        let mail_root = runtime
            .resolve_account(runtime.account.clone())
            .unwrap()
            .mail_path(&runtime.config);
        queue::enqueue(&mail_root, &stale).unwrap();

        let err = runtime
            .queue_run(vec![stale.id.clone()], false)
            .await
            .unwrap_err();
        assert!(err.to_string().contains("policy"));
        assert!(err.to_string().contains("read-only"));

        let loaded = queue::load(&mail_root, &stale.id).unwrap();
        assert_eq!(loaded.status, QueueStatus::Failed);
        assert!(loaded.error.unwrap().contains("policy"));
    }

    #[tokio::test]
    async fn queue_run_rejects_stale_send_under_archive() {
        let tmp = tempfile::tempdir().unwrap();
        let runtime = test_runtime(tmp.path(), MutationPolicy::Archive);

        let stale = QueueItem::new(
            "test".into(),
            QueuedCommand::Send {
                path: std::path::PathBuf::from("old.eml"),
                from: None,
            },
        );
        let mail_root = runtime
            .resolve_account(runtime.account.clone())
            .unwrap()
            .mail_path(&runtime.config);
        queue::enqueue(&mail_root, &stale).unwrap();

        let err = runtime
            .queue_run(vec![stale.id.clone()], false)
            .await
            .unwrap_err();
        assert!(err.to_string().contains("policy"));
        assert!(err.to_string().contains("send"));

        let loaded = queue::load(&mail_root, &stale.id).unwrap();
        assert_eq!(loaded.status, QueueStatus::Failed);
    }

    #[tokio::test]
    async fn execute_queued_rejects_delete_under_read_only() {
        let tmp = tempfile::tempdir().unwrap();
        let runtime = test_runtime(tmp.path(), MutationPolicy::ReadOnly);

        let cmd = QueuedCommand::Delete {
            handles: vec!["h1".into()],
            expunge: false,
            confirm: true,
        };
        let err = runtime.execute_queued(cmd, false).await.unwrap_err();
        assert!(matches!(err, VivariumError::Policy(_)));
        assert!(err.to_string().contains("read-only"));
    }

    #[tokio::test]
    async fn execute_queued_rejects_send_under_read_only() {
        let tmp = tempfile::tempdir().unwrap();
        let runtime = test_runtime(tmp.path(), MutationPolicy::ReadOnly);

        let cmd = QueuedCommand::Send {
            path: std::path::PathBuf::from("test.eml"),
            from: None,
        };
        let err = runtime.execute_queued(cmd, false).await.unwrap_err();
        assert!(matches!(err, VivariumError::Policy(_)));
    }

    #[test]
    fn enqueue_admission_rejects_delete_under_read_only() {
        let tmp = tempfile::tempdir().unwrap();
        let runtime = test_runtime(tmp.path(), MutationPolicy::ReadOnly);

        let err = runtime
            .enqueue(EnqueueCommand::Delete {
                handles: vec!["h1".into()],
                trash: false,
                expunge: false,
                confirm: false,
            })
            .unwrap_err();
        assert!(matches!(err, VivariumError::Policy(_)));

        // Verify nothing was persisted to disk.
        let mail_root = runtime
            .resolve_account(runtime.account.clone())
            .unwrap()
            .mail_path(&runtime.config);
        assert!(queue::pending_ids(&mail_root).unwrap().is_empty());
    }

    #[test]
    fn enqueue_admission_allows_archive_under_full_write() {
        let tmp = tempfile::tempdir().unwrap();
        let runtime = test_runtime(tmp.path(), MutationPolicy::FullWrite);

        runtime
            .enqueue(EnqueueCommand::Archive {
                handles: vec!["h1".into()],
            })
            .unwrap();

        let mail_root = runtime
            .resolve_account(runtime.account.clone())
            .unwrap()
            .mail_path(&runtime.config);
        assert_eq!(queue::pending_ids(&mail_root).unwrap().len(), 1);
    }
}

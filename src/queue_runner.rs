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
#[path = "queue_runner_test.rs"]
mod tests;

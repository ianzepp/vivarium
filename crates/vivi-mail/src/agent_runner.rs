use crate::VivariumError;
use crate::agent::{AgentPollOptions, poll};
use crate::cli::{AgentArgs, AgentCommand, MailCommand};
use crate::config::{Account, Config};
use crate::store::MailStore;

pub enum AgentDispatch {
    Handled,
    Unhandled(Box<MailCommand>),
}

pub trait AgentRunner {
    fn run_agent_command(&self, command: MailCommand) -> Result<AgentDispatch, VivariumError>;
}

pub struct AgentContext<'a> {
    pub config: &'a Config,
    pub account: &'a Account,
}

impl AgentRunner for AgentContext<'_> {
    fn run_agent_command(&self, command: MailCommand) -> Result<AgentDispatch, VivariumError> {
        let MailCommand::Agent(AgentArgs { command }) = command else {
            return Ok(AgentDispatch::Unhandled(Box::new(command)));
        };
        match command {
            AgentCommand::Poll {
                from_addr,
                folder,
                dry_run,
                json,
                codex_command,
                codex_args,
            } => {
                let store = MailStore::new(&self.account.mail_path(self.config));
                poll(
                    &store,
                    &self.account.name,
                    AgentPollOptions {
                        trusted_from: from_addr,
                        folder,
                        dry_run,
                        json,
                        codex_command,
                        codex_args,
                    },
                )?;
                Ok(AgentDispatch::Handled)
            }
            AgentCommand::Archive { .. }
            | AgentCommand::Delete { .. }
            | AgentCommand::Move { .. }
            | AgentCommand::Flag { .. } => Ok(AgentDispatch::Unhandled(Box::new(
                MailCommand::Agent(AgentArgs { command }),
            ))),
        }
    }
}

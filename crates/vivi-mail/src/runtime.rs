//! Mail runtime: credentials, account selection, and command dispatch.
//!
//! The runtime owns the loaded `config.toml` and `accounts.toml` and every
//! method that talks to a mail provider or the local archive. It is a plain
//! library type so the `vivi` binary can build it and route mail commands here.

use std::path::PathBuf;

use crate::VivariumError;
use crate::agent_runner::{self, AgentRunner};
#[cfg(feature = "outbox")]
use crate::cli::{AuthArgs, TokenArgs};
use crate::cli::{
    DoctorArgs, ExportArgs, FoldersArgs, IndexArgs, MailCommand, ProtonArgs, SearchArgs, ShowArgs,
    ThreadArgs, WatchInboxArgs,
};
use crate::config::{Account, AccountsFile, Config};
use crate::draft_runner::DraftDispatch;
use crate::label_runner::LabelDispatch;
use crate::queue_runner::QueueDispatch;
use crate::store::MailStore;
use crate::{sync_command, sync_events_command};

/// Shared state for every mail command.
pub struct Runtime {
    pub(crate) config: Config,
    pub(crate) accounts: AccountsFile,
    pub(crate) account: Option<String>,
    pub(crate) insecure: bool,
}

#[allow(clippy::struct_excessive_bools)]
struct SearchRequest<'a> {
    query: &'a str,
    folder: Option<&'a str>,
    from_addr: Option<&'a str>,
    from_domain: Option<&'a str>,
    limit: usize,
    offset: usize,
    as_json: bool,
    count: bool,
    semantic: bool,
    hybrid: bool,
}

impl Runtime {
    /// Load `config.toml` and `accounts.toml` for a run.
    ///
    /// # Errors
    /// Returns an error if either file is missing or malformed, or if
    /// `accounts.toml` is group/world readable without `ignore_permissions`.
    pub fn load(
        config_path: Option<PathBuf>,
        account: Option<String>,
        insecure: bool,
        ignore_permissions: bool,
    ) -> Result<Self, VivariumError> {
        let config_path = config_path.unwrap_or_else(Config::default_path);
        let config = Config::load(&config_path)?;
        let accounts_file =
            AccountsFile::load_with_options(&AccountsFile::default_path(), ignore_permissions)?;
        if insecure {
            tracing::warn!("accepting invalid TLS certificates because --insecure was provided");
        }
        Ok(Self {
            config,
            accounts: accounts_file,
            account,
            insecure,
        })
    }

    /// Dispatch one mail command.
    ///
    /// # Errors
    /// Returns an error if any command fails.
    pub async fn run(&self, command: MailCommand) -> Result<(), VivariumError> {
        let Some(command) = self.dispatch_write_command(command).await? else {
            return Ok(());
        };
        match command {
            MailCommand::Init => crate::init::run_init(),
            #[cfg(feature = "outbox")]
            MailCommand::Auth(AuthArgs {
                account,
                client_id,
                client_secret,
            }) => self.auth(account, client_id, client_secret).await,
            #[cfg(feature = "outbox")]
            MailCommand::Token(TokenArgs { account }) => self.token(account).await,
            command @ MailCommand::Sync(_) => self.run_sync_command(command).await,
            command @ MailCommand::SyncEvents(_) => self.run_sync_events_command(command).await,
            MailCommand::Folders(FoldersArgs { account, json }) => {
                self.folders(account, json).await
            }
            MailCommand::Doctor(DoctorArgs { account, json }) => self.doctor(account, json).await,
            MailCommand::Proton(ProtonArgs { command }) => self.proton_command(command).await,
            MailCommand::Render(command) => self.render(command),
            command @ MailCommand::List(_) => self.run_list_command(command),
            MailCommand::Show(ShowArgs { message_ids, json }) => self.show(&message_ids, json),
            MailCommand::Thread(ThreadArgs {
                message_id,
                json,
                limit,
            }) => self.thread(&message_id, json, limit),
            MailCommand::Export(ExportArgs { message_id, text }) => self.export(&message_id, text),
            command @ MailCommand::Search(_) => self.run_search_command(command).await,
            MailCommand::Index(IndexArgs { command }) => self.index(command).await,
            MailCommand::WatchInbox(WatchInboxArgs { account, json }) => {
                self.watch_inbox(account, json).await
            }
            // Consumed by dispatch_write_command above.
            command @ (MailCommand::Agent(_)
            | MailCommand::Reply(_)
            | MailCommand::Compose(_)
            | MailCommand::Exec(_)
            | MailCommand::Enqueue(_)
            | MailCommand::Queue(_)
            | MailCommand::Labels(_)
            | MailCommand::Label(_)) => {
                let _ = command;
                unreachable!()
            }
        }
    }

    async fn dispatch_write_command(
        &self,
        command: MailCommand,
    ) -> Result<Option<MailCommand>, VivariumError> {
        let command = match self.run_queue_command(command).await? {
            QueueDispatch::Handled => return Ok(None),
            QueueDispatch::Unhandled(command) => *command,
        };
        let command = match self.run_label_command(command)? {
            LabelDispatch::Handled => return Ok(None),
            LabelDispatch::Unhandled(command) => *command,
        };
        let command = match self.run_agent_command(command)? {
            AgentDispatchOutcome::Handled => return Ok(None),
            AgentDispatchOutcome::Unhandled(command) => *command,
        };
        match self.run_draft_command(command).await? {
            DraftDispatch::Handled => Ok(None),
            DraftDispatch::Unhandled(command) => Ok(Some(*command)),
        }
    }

    fn run_agent_command(
        &self,
        command: MailCommand,
    ) -> Result<AgentDispatchOutcome, VivariumError> {
        let acct = self.resolve_account(self.account.clone())?;
        let context = agent_runner::AgentContext {
            config: &self.config,
            account: &acct,
        };
        match context.run_agent_command(command)? {
            agent_runner::AgentDispatch::Handled => Ok(AgentDispatchOutcome::Handled),
            agent_runner::AgentDispatch::Unhandled(command) => {
                Ok(AgentDispatchOutcome::Unhandled(command))
            }
        }
    }

    pub(crate) fn resolve_account(&self, name: Option<String>) -> Result<Account, VivariumError> {
        if let Some(n) = name {
            Ok(self.accounts.find_account(&n)?.clone())
        } else {
            let first = self
                .accounts
                .accounts
                .first()
                .ok_or_else(|| VivariumError::Config("no accounts configured".into()))?;
            Ok(first.clone())
        }
    }

    pub(crate) fn selected_account_name(&self, name: Option<String>) -> Option<String> {
        name.or_else(|| self.account.clone())
    }

    async fn run_sync_command(&self, command: MailCommand) -> Result<(), VivariumError> {
        self.sync(sync_command::SyncOptions::from_command(command))
            .await
    }

    async fn run_sync_events_command(&self, command: MailCommand) -> Result<(), VivariumError> {
        self.sync_events(sync_events_command::SyncEventsOptions::from_command(
            command,
        ))
        .await
    }

    async fn run_search_command(&self, command: MailCommand) -> Result<(), VivariumError> {
        let MailCommand::Search(SearchArgs {
            query,
            folder,
            from_addr,
            from_domain,
            limit,
            offset,
            json,
            count,
            semantic,
            hybrid,
        }) = command
        else {
            unreachable!();
        };
        self.search(SearchRequest {
            query: &query,
            folder: folder.as_deref(),
            from_addr: from_addr.as_deref(),
            from_domain: from_domain.as_deref(),
            limit,
            offset,
            as_json: json,
            count,
            semantic,
            hybrid,
        })
        .await
    }

    #[cfg(feature = "outbox")]
    async fn auth(
        &self,
        account: Option<String>,
        client_id: Option<String>,
        client_secret: Option<String>,
    ) -> Result<(), VivariumError> {
        let acct = self.resolve_account(self.selected_account_name(account))?;
        let client = crate::oauth::oauth_client(&acct, client_id, client_secret)?;
        crate::oauth::authorize(&acct, client).await
    }

    #[cfg(feature = "outbox")]
    async fn token(&self, account: Option<String>) -> Result<(), VivariumError> {
        let acct = self.resolve_account(self.selected_account_name(account))?;
        let client = crate::oauth::oauth_client(&acct, None, None)?;
        crate::oauth::print_access_token(&acct, client).await
    }

    fn show(&self, message_ids: &[String], as_json: bool) -> Result<(), VivariumError> {
        let acct = self.resolve_account(self.account.clone())?;
        let store = MailStore::new(&acct.mail_path(&self.config));
        if as_json {
            return crate::retrieve::print_json_messages(&store, &acct.name, message_ids);
        }
        for (i, message_id) in message_ids.iter().enumerate() {
            if i > 0 {
                println!("\n---\n");
            }
            let data = store.read_message(message_id)?;
            let output = crate::message::render_message(&data)?;
            println!("{output}");
        }
        Ok(())
    }

    fn thread(&self, message_id: &str, as_json: bool, limit: usize) -> Result<(), VivariumError> {
        if !as_json {
            return Err(VivariumError::Message(
                "thread currently supports JSON output only; pass --json".into(),
            ));
        }
        let acct = self.resolve_account(self.account.clone())?;
        let store = MailStore::new(&acct.mail_path(&self.config));
        crate::thread::print_thread_json(&store, &acct.name, message_id, limit)
    }

    fn export(&self, message_id: &str, as_text: bool) -> Result<(), VivariumError> {
        let acct = self.resolve_account(self.account.clone())?;
        let store = MailStore::new(&acct.mail_path(&self.config));
        if as_text {
            crate::retrieve::export_text_message(&store, message_id)
        } else {
            crate::retrieve::export_raw_message(&store, message_id)
        }
    }

    async fn watch_inbox(&self, account: Option<String>, json: bool) -> Result<(), VivariumError> {
        if !json {
            return Err(VivariumError::Message(
                "watch-inbox requires --json for the stable Ops event contract".into(),
            ));
        }
        let name = self
            .selected_account_name(account)
            .ok_or_else(|| VivariumError::Config("watch-inbox requires --account".into()))?;
        let acct = self.accounts.find_account(&name)?;
        crate::watch_inbox::watch_inbox(acct, &self.config, self.insecure).await
    }

    async fn search(&self, request: SearchRequest<'_>) -> Result<(), VivariumError> {
        let acct = self.resolve_account(self.account.clone())?;
        let mail_root = acct.mail_path(&self.config);
        let folder = request
            .folder
            .map(crate::search::canonical_search_folder)
            .transpose()?;
        let filters = crate::search::SearchFilters::new(
            folder.as_deref(),
            request.from_addr,
            request.from_domain,
        );
        let (results, total) = if request.semantic || request.hybrid {
            crate::search::semantic_or_hybrid_search(
                &self.config,
                &mail_root,
                &acct.name,
                request.query,
                crate::search::SemanticSearchOptions {
                    limit: request.limit,
                    offset: request.offset,
                    semantic: request.semantic,
                    hybrid: request.hybrid,
                    filters,
                },
            )
            .await?
        } else {
            crate::search::keyword_search(
                &mail_root,
                &acct.name,
                request.query,
                request.limit,
                request.offset,
                filters,
            )?
        };
        crate::search::print_search_output(crate::search::SearchOutput {
            query: request.query,
            folder: folder.as_deref(),
            limit: request.limit,
            offset: request.offset,
            results,
            total,
            as_json: request.as_json,
            count_only: request.count,
        });
        Ok(())
    }
}

/// Outcome of the agent dispatch stage, before the remaining write handlers run.
pub(crate) enum AgentDispatchOutcome {
    Handled,
    Unhandled(Box<MailCommand>),
}

pub(crate) fn print_sync_result(account: &str, result: &crate::sync::SyncResult) {
    println!(
        "synced {account}: {} new messages, {} cataloged, {} extracted, {} extraction errors, {} decryption errors",
        result.new,
        result.cataloged,
        result.extracted,
        result.extraction_errors,
        result.decryption_errors
    );
}

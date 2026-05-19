use std::process;

use clap::Parser;
use tracing_subscriber::EnvFilter;

use vivarium::VivariumError;
use vivarium::cli::{Cli, Command};
use vivarium::config::{Account, AccountsFile, Config};
use vivarium::message;
use vivarium::store::MailStore;

mod agent_runner;
mod doctor_command;
mod draft_runner;
mod folders_command;
mod index_runner;
mod label_runner;
mod list_runner;
mod local_mailspace_command;
mod local_mailspace_dump;
mod mutation_runner;
mod proton_api_command;
mod proton_fixture_command;
mod queue_runner;
mod sync_command;
mod sync_events_command;

use agent_runner::{AgentContext, AgentDispatch, AgentRunner};
use draft_runner::DraftDispatch;
use label_runner::LabelDispatch;
use local_mailspace_command::run_mailspace_command;
use queue_runner::QueueDispatch;

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

#[tokio::main]
async fn main() {
    let cli = Cli::parse();

    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| {
            if cli.verbose {
                EnvFilter::new("vivarium=debug")
            } else {
                EnvFilter::new("vivarium=info")
            }
        }))
        .init();

    if let Err(e) = run(cli).await {
        eprintln!("error: {e}");
        process::exit(1);
    }
}

async fn run(cli: Cli) -> Result<(), VivariumError> {
    if let Command::Init = cli.command {
        return vivarium::init::run_init();
    }
    if run_mailspace_command(&cli.command)? {
        return Ok(());
    }

    let runtime = Runtime::load(&cli)?;
    runtime.run(cli.command).await
}

struct Runtime {
    config: Config,
    accounts: AccountsFile,
    account: Option<String>,
    insecure: bool,
}

impl Runtime {
    fn load(cli: &Cli) -> Result<Self, VivariumError> {
        let config_path = cli.config.clone().unwrap_or_else(Config::default_path);
        let config = Config::load(&config_path)?;
        let accounts_file =
            AccountsFile::load_with_options(&AccountsFile::default_path(), cli.ignore_permissions)?;
        if cli.insecure {
            tracing::warn!("accepting invalid TLS certificates because --insecure was provided");
        }
        Ok(Self {
            config,
            accounts: accounts_file,
            account: cli.account.clone(),
            insecure: cli.insecure,
        })
    }

    async fn run(&self, command: Command) -> Result<(), VivariumError> {
        let Some(command) = self.dispatch_write_command(command).await? else {
            return Ok(());
        };
        match command {
            Command::Init => unreachable!(),
            #[cfg(feature = "outbox")]
            Command::Auth {
                account,
                client_id,
                client_secret,
            } => self.auth(account, client_id, client_secret).await,
            #[cfg(feature = "outbox")]
            Command::Token { account } => self.token(account).await,
            command @ Command::Sync { .. } => self.run_sync_command(command).await,
            command @ Command::SyncEvents { .. } => self.run_sync_events_command(command).await,
            Command::Folders { account, json } => self.folders(account, json).await,
            Command::Doctor { account, json } => self.doctor(account, json).await,
            Command::Proton { command } => self.proton_command(command).await,
            command @ Command::List { .. } => self.run_list_command(command),
            Command::Mailspace { .. } | Command::Mail { .. } | Command::Task { .. } => {
                unreachable!()
            }
            Command::Show { message_ids, json } => self.show(&message_ids, json),
            Command::Thread {
                message_id,
                json,
                limit,
            } => self.thread(&message_id, json, limit),
            Command::Export { message_id, text } => self.export(&message_id, text),
            command @ Command::Search { .. } => self.run_search_command(command).await,
            Command::Index { command } => self.index(command).await,
            Command::Agent { .. } => unreachable!(),
            #[cfg(feature = "outbox")]
            Command::Watch { account } => self.watch(account).await,
            Command::Reply(_) | Command::Compose(_) => unreachable!(),
            Command::Exec { .. } | Command::Enqueue { .. } | Command::Queue { .. } => {
                unreachable!()
            }
            Command::Labels { .. } | Command::Label { .. } => unreachable!(),
        }
    }

    async fn dispatch_write_command(
        &self,
        command: Command,
    ) -> Result<Option<Command>, VivariumError> {
        let command = match self.run_queue_command(command).await? {
            QueueDispatch::Handled => return Ok(None),
            QueueDispatch::Unhandled(command) => command,
        };
        let command = match self.run_label_command(command).await? {
            LabelDispatch::Handled => return Ok(None),
            LabelDispatch::Unhandled(command) => command,
        };
        let command = match self.run_agent_command(command)? {
            AgentDispatch::Handled => return Ok(None),
            AgentDispatch::Unhandled(command) => command,
        };
        match self.run_draft_command(command).await? {
            DraftDispatch::Handled => Ok(None),
            DraftDispatch::Unhandled(command) => Ok(Some(command)),
        }
    }

    fn run_agent_command(&self, command: Command) -> Result<AgentDispatch, VivariumError> {
        let acct = self.resolve_account(self.account.clone())?;
        AgentContext {
            config: &self.config,
            account: &acct,
        }
        .run_agent_command(command)
    }

    fn resolve_account(&self, name: Option<String>) -> Result<Account, VivariumError> {
        match name {
            Some(n) => Ok(self.accounts.find_account(&n)?.clone()),
            None => {
                let first = self
                    .accounts
                    .accounts
                    .first()
                    .ok_or_else(|| VivariumError::Config("no accounts configured".into()))?;
                Ok(first.clone())
            }
        }
    }

    fn selected_account_name(&self, name: Option<String>) -> Option<String> {
        name.or_else(|| self.account.clone())
    }

    async fn run_sync_command(&self, command: Command) -> Result<(), VivariumError> {
        self.sync(sync_command::SyncOptions::from_command(command))
            .await
    }

    async fn run_sync_events_command(&self, command: Command) -> Result<(), VivariumError> {
        self.sync_events(sync_events_command::SyncEventsOptions::from_command(
            command,
        ))
        .await
    }

    async fn run_search_command(&self, command: Command) -> Result<(), VivariumError> {
        let Command::Search {
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
        } = command
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
        let client = vivarium::oauth::oauth_client(&acct, client_id, client_secret)?;
        vivarium::oauth::authorize(&acct, client).await
    }

    #[cfg(feature = "outbox")]
    async fn token(&self, account: Option<String>) -> Result<(), VivariumError> {
        let acct = self.resolve_account(self.selected_account_name(account))?;
        let client = vivarium::oauth::oauth_client(&acct, None, None)?;
        vivarium::oauth::print_access_token(&acct, client).await
    }

    fn show(&self, message_ids: &[String], as_json: bool) -> Result<(), VivariumError> {
        let acct = self.resolve_account(self.account.clone())?;
        let store = MailStore::new(&acct.mail_path(&self.config));
        if as_json {
            return vivarium::retrieve::print_json_messages(&store, &acct.name, message_ids);
        }
        for (i, message_id) in message_ids.iter().enumerate() {
            if i > 0 {
                println!("\n---\n");
            }
            let data = store.read_message(message_id)?;
            let output = message::render_message(&data)?;
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
        vivarium::thread::print_thread_json(&store, &acct.name, message_id, limit)
    }

    fn export(&self, message_id: &str, as_text: bool) -> Result<(), VivariumError> {
        let acct = self.resolve_account(self.account.clone())?;
        let store = MailStore::new(&acct.mail_path(&self.config));
        if as_text {
            vivarium::retrieve::export_text_message(&store, message_id)
        } else {
            vivarium::retrieve::export_raw_message(&store, message_id)
        }
    }

    #[cfg(feature = "outbox")]
    async fn watch(&self, account: Option<String>) -> Result<(), VivariumError> {
        match self.selected_account_name(account) {
            Some(name) => {
                let acct = self.accounts.find_account(&name)?;
                vivarium::watch::watch_account(acct, &self.config, self.insecure).await
            }
            None => {
                vivarium::watch::watch_all(&self.accounts.accounts, &self.config, self.insecure)
                    .await
            }
        }
    }

    async fn search(&self, request: SearchRequest<'_>) -> Result<(), VivariumError> {
        let acct = self.resolve_account(self.account.clone())?;
        let mail_root = acct.mail_path(&self.config);
        let folder = request
            .folder
            .map(vivarium::search::canonical_search_folder)
            .transpose()?;
        let filters = vivarium::search::SearchFilters::new(
            folder.as_deref(),
            request.from_addr,
            request.from_domain,
        );
        let (results, total) = if request.semantic || request.hybrid {
            vivarium::search::semantic_or_hybrid_search(
                &self.config,
                &mail_root,
                &acct.name,
                request.query,
                vivarium::search::SemanticSearchOptions {
                    limit: request.limit,
                    offset: request.offset,
                    semantic: request.semantic,
                    hybrid: request.hybrid,
                    filters,
                },
            )
            .await?
        } else {
            vivarium::search::keyword_search(
                &mail_root,
                &acct.name,
                request.query,
                request.limit,
                request.offset,
                filters,
            )?
        };
        vivarium::search::print_search_output(vivarium::search::SearchOutput {
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

fn print_sync_result(account: &str, result: &vivarium::sync::SyncResult) {
    println!(
        "synced {account}: {} new messages, {} cataloged, {} extracted, {} extraction errors, {} decryption errors",
        result.new,
        result.cataloged,
        result.extracted,
        result.extraction_errors,
        result.decryption_errors
    );
}

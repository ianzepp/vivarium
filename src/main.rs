use std::process;

use clap::Parser;
use tracing_subscriber::EnvFilter;

use vivarium::VivariumError;
use vivarium::cli::{Cli, Command, IndexCommand};
use vivarium::config::{Account, AccountsFile, Config};
use vivarium::message;
use vivarium::store::MailStore;

mod agent_runner;
mod draft_runner;
mod folders_command;
mod label_runner;
mod mutation_runner;
mod sync_command;

use agent_runner::AgentDispatch;
use draft_runner::DraftDispatch;
use label_runner::LabelDispatch;
use mutation_runner::MutationDispatch;

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
            Command::Sync {
                account,
                limit,
                since,
                before,
                reset,
            } => self.sync(account, limit, since, before, reset).await,
            Command::Folders { account, json } => self.folders(account, json).await,
            Command::List {
                folder,
                limit,
                since,
                before,
            } => self.list(&folder, limit, since, before),
            Command::Show { message_ids, json } => self.show(&message_ids, json),
            Command::Thread {
                message_id,
                json,
                limit,
            } => self.thread(&message_id, json, limit),
            Command::Archive { .. }
            | Command::Delete { .. }
            | Command::Move { .. }
            | Command::Flag { .. } => unreachable!(),
            Command::Export { message_id, text } => self.export(&message_id, text),
            Command::Search {
                query,
                limit,
                offset,
                json,
            } => self.search(&query, limit, offset, json),
            Command::Index { command } => self.index(command),
            #[cfg(feature = "outbox")]
            Command::Watch { account } => self.watch(account).await,
            Command::Send { .. } | Command::Reply { .. } | Command::Compose { .. } => {
                unreachable!()
            }
            Command::Agent { .. } => unreachable!(),
            Command::Labels { .. } | Command::Label { .. } => unreachable!(),
        }
    }

    async fn dispatch_write_command(
        &self,
        command: Command,
    ) -> Result<Option<Command>, VivariumError> {
        let command = match self.run_agent_command(command).await? {
            AgentDispatch::Handled => return Ok(None),
            AgentDispatch::Unhandled(command) => command,
        };
        let command = match self.run_label_command(command).await? {
            LabelDispatch::Handled => return Ok(None),
            LabelDispatch::Unhandled(command) => command,
        };
        let command = match self.run_mutation_command(command).await? {
            MutationDispatch::Handled => return Ok(None),
            MutationDispatch::Unhandled(command) => command,
        };
        match self.run_draft_command(command).await? {
            DraftDispatch::Handled => Ok(None),
            DraftDispatch::Unhandled(command) => Ok(Some(command)),
        }
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

    fn list(
        &self,
        folder: &str,
        limit: Option<usize>,
        since: Option<String>,
        before: Option<String>,
    ) -> Result<(), VivariumError> {
        let window = vivarium::sync::SyncWindow::parse(since.as_deref(), before.as_deref())?;
        let accounts = match &self.account {
            Some(name) => vec![self.accounts.find_account(name)?.clone()],
            None => self.accounts.accounts.clone(),
        };
        for acct in &accounts {
            println!("# {}", acct.name);
            let store = MailStore::new(&acct.mail_path(&self.config));
            let entries = store.list_messages(folder)?;
            let entries = vivarium::list::filter_entries(entries, window, limit);
            vivarium::list::print_entries(folder, &entries);
        }
        Ok(())
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

    fn search(
        &self,
        query: &str,
        limit: usize,
        offset: usize,
        as_json: bool,
    ) -> Result<(), VivariumError> {
        let acct = self.resolve_account(self.account.clone())?;
        let mail_root = acct.mail_path(&self.config);

        let (results, total) =
            vivarium::search::keyword_search(&mail_root, &acct.name, query, limit, offset)?;
        vivarium::search::print_results(query, limit, offset, results, total, as_json);
        Ok(())
    }

    fn index(&self, command: IndexCommand) -> Result<(), VivariumError> {
        let acct = self.resolve_account(self.account.clone())?;
        let mail_root = acct.mail_path(&self.config);
        match command {
            IndexCommand::Rebuild => {
                let stats = vivarium::email_index::rebuild(&mail_root, &acct.name)?;
                println!(
                    "indexed {}: scanned={} updated={} reused={} stale={} errors={}",
                    acct.name,
                    stats.scanned,
                    stats.updated,
                    stats.reused,
                    stats.stale,
                    stats.errors
                );
                Ok(())
            }
            IndexCommand::Status => {
                let catalog = vivarium::catalog::Catalog::open(&mail_root)?;
                let catalog_count = catalog.count_messages(&acct.name)?;
                let index = vivarium::email_index::EmailIndex::open(&mail_root)?;
                let indexed_count = index.count_messages(&acct.name)?;
                println!(
                    "index {}: catalog={} indexed={} pending={}",
                    acct.name,
                    catalog_count,
                    indexed_count,
                    catalog_count.saturating_sub(indexed_count)
                );
                Ok(())
            }
            IndexCommand::Pending => {
                let catalog = vivarium::catalog::Catalog::open(&mail_root)?;
                let catalog_count = catalog.count_messages(&acct.name)?;
                let index = vivarium::email_index::EmailIndex::open(&mail_root)?;
                let indexed_count = index.count_messages(&acct.name)?;
                println!(
                    "pending {}: {}",
                    acct.name,
                    catalog_count.saturating_sub(indexed_count)
                );
                Ok(())
            }
        }
    }
}

fn print_sync_result(account: &str, result: &vivarium::sync::SyncResult) {
    println!(
        "synced {account}: {} new messages, {} cataloged, {} extracted, {} extraction errors",
        result.new, result.cataloged, result.extracted, result.extraction_errors
    );
}

use std::process;

use clap::Parser;
use tracing_subscriber::EnvFilter;

use vivarium::VivariumError;
use vivarium::cli::{Cli, Command};
use vivarium::config::{Account, AccountsFile, Config};
use vivarium::message::{self, MessageEntry};
use vivarium::store::MailStore;

#[cfg(feature = "outbox")]
use vivarium::smtp;
#[cfg(feature = "outbox")]
use vivarium::watch;
#[cfg(feature = "outbox")]
use vivarium::outbox;

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
            Command::Sync { account } => self.sync(account).await,
            Command::List { folder } => self.list(&folder),
            Command::Show { message_ids } => self.show(&message_ids),
            Command::Archive { message_ids } => self.archive(&message_ids),
            Command::Search { query, limit, offset, json } => self.search(&query, limit, offset, json),
            #[cfg(feature = "outbox")]
            Command::Watch { account } => self.watch(account).await,
            #[cfg(feature = "outbox")]
            Command::Send { path } => self.send(&path).await,
            #[cfg(feature = "outbox")]
            Command::Reply { message_id, body } => self.reply(&message_id, body).await,
            #[cfg(feature = "outbox")]
            Command::Compose { to, subject } => self.compose(&to, &subject),
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

    async fn sync(&self, account: Option<String>) -> Result<(), VivariumError> {
        match self.selected_account_name(account) {
            Some(name) => {
                let acct = self.accounts.find_account(&name)?;
                let result =
                    vivarium::sync::sync_account(acct, &self.config, self.insecure).await?;
                println!("synced {}: {} new messages", name, result.new);
            }
            None => {
                for acct in &self.accounts.accounts {
                    let result =
                        vivarium::sync::sync_account(acct, &self.config, self.insecure).await?;
                    println!("synced {}: {} new messages", acct.name, result.new);
                }
            }
        }
        Ok(())
    }

    fn list(&self, folder: &str) -> Result<(), VivariumError> {
        let accounts = match &self.account {
            Some(name) => vec![self.accounts.find_account(name)?.clone()],
            None => self.accounts.accounts.clone(),
        };
        for acct in &accounts {
            println!("# {}", acct.name);
            let store = MailStore::new(&acct.mail_path(&self.config));
            let entries = store.list_messages(folder)?;
            print_entries(folder, &entries);
        }
        Ok(())
    }

    fn show(&self, message_ids: &[String]) -> Result<(), VivariumError> {
        let acct = self.resolve_account(self.account.clone())?;
        let store = MailStore::new(&acct.mail_path(&self.config));
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

    fn archive(&self, message_ids: &[String]) -> Result<(), VivariumError> {
        let acct = self.resolve_account(self.account.clone())?;
        let store = MailStore::new(&acct.mail_path(&self.config));
        for message_id in message_ids {
            store.move_message(message_id, "inbox", "archive")?;
            println!("archived {message_id}");
        }
        Ok(())
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

    #[cfg(feature = "outbox")]
    async fn send(&self, path: &Path) -> Result<(), VivariumError> {
        let acct = self.resolve_account(self.account.clone())?;
        let data = std::fs::read(path)?;
        let reject_invalid_certs = acct.reject_invalid_certs(&self.config) && !self.insecure;
        vivarium::smtp::send_raw(&acct, &data, reject_invalid_certs).await?;
        println!("sent {}", path.display());
        Ok(())
    }

    #[cfg(feature = "outbox")]
    async fn reply(&self, message_id: &str, body: Option<String>) -> Result<(), VivariumError> {
        let acct = self.resolve_account(self.account.clone())?;
        let store = MailStore::new(&acct.mail_path(&self.config));
        let original = store.read_message(message_id)?;
        let Some(reply_eml) = reply_message(&original, body, &acct.email)? else {
            return Ok(());
        };
        let reject_invalid_certs = acct.reject_invalid_certs(&self.config) && !self.insecure;
        vivarium::smtp::send_raw(&acct, reply_eml.as_bytes(), reject_invalid_certs).await?;
        println!("replied to {message_id}");
        Ok(())
    }

    #[cfg(feature = "outbox")]
    fn compose(&self, to: &str, subject: &str) -> Result<(), VivariumError> {
        let acct = self.resolve_account(self.account.clone())?;
        let store = MailStore::new(&acct.mail_path(&self.config));
        let draft = format!(
            "From: {}\r\nTo: {to}\r\nSubject: {subject}\r\n\r\n",
            acct.email
        );
        let Some(edited) = edit_message("compose", draft.as_bytes())? else {
            println!("compose cancelled");
            return Ok(());
        };
        message::validate_message_headers(&edited)?;
        let draft_id = format!("draft-{}", chrono::Utc::now().timestamp());
        let path = store.store_message("drafts", &draft_id, &edited)?;
        println!("draft created: {}", path.display());
        println!(
            "edit the file, then send with: vivarium send {}",
            path.display()
        );
        Ok(())
    }

    fn search(&self, query: &str, limit: usize, offset: usize, as_json: bool) -> Result<(), VivariumError> {
        let acct = self.resolve_account(self.account.clone())?;
        let mail_root = acct.mail_path(&self.config);

        let (results, total) = vivarium::search::keyword_search(&mail_root, query, limit, offset)?;

        if as_json {
            let output = serde_json::json!({
                "query": query,
                "total": total,
                "limit": limit,
                "offset": offset,
                "results": results.into_iter()
                    .map(|r: vivarium::search::SearchResult| vivarium::search::to_json_result(&r))
                    .collect::<Vec<_>>(),
            });
            println!("{}", serde_json::to_string_pretty(&output).unwrap_or_else(|_| output.to_string()));
        } else {
            println!("search: {} results for '{}'", total, query);
            if results.is_empty() {
                return Ok(());
            }
            for r in &results {
                println!("  {}  {:<16}  {}  {}", r.handle, r.date, r.from, r.subject);
                if !r.snippet.is_empty() {
                    println!("    snippet: {}", r.snippet);
                }
            }
        }
        Ok(())
    }
}

fn print_entries(folder: &str, entries: &[MessageEntry]) {
    if entries.is_empty() {
        println!("  no messages in {folder}");
    } else {
        for entry in entries {
            println!("  {entry}");
        }
    }
    println!();
}

#[cfg(feature = "outbox")]
fn reply_message(
    original: &[u8],
    body: Option<String>,
    from: &str,
) -> Result<Option<String>, VivariumError> {
    match body {
        Some(body) => message::build_reply(original, &body, from).map(Some),
        None => edit_reply(original, from),
    }
}

#[cfg(feature = "outbox")]
fn edit_reply(original: &[u8], from: &str) -> Result<Option<String>, VivariumError> {
    let template = message::build_reply_template(original, from)?;
    let Some(edited) = edit_message("reply", template.as_bytes())? else {
        println!("reply cancelled");
        return Ok(None);
    };
    message::validate_message_headers(&edited)?;
    String::from_utf8(edited)
        .map(Some)
        .map_err(|e| VivariumError::Message(format!("edited reply is not UTF-8: {e}")))
}

#[cfg(feature = "outbox")]
fn edit_message(prefix: &str, initial: &[u8]) -> Result<Option<Vec<u8>>, VivariumError> {
    let path = editor_temp_path(prefix);
    std::fs::write(&path, initial)?;

    let editor = std::env::var("VISUAL")
        .or_else(|_| std::env::var("EDITOR"))
        .unwrap_or_else(|_| "vi".to_string());
    let status = process::Command::new("sh")
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

#[cfg(feature = "outbox")]
fn editor_temp_path(prefix: &str) -> PathBuf {
    let unique = format!(
        "vivarium-{prefix}-{}-{}.eml",
        process::id(),
        chrono::Utc::now().timestamp_nanos_opt().unwrap_or_default()
    );
    std::env::temp_dir().join(Path::new(&unique))
}

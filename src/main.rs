use std::process;

use clap::Parser;
use tracing_subscriber::EnvFilter;

use vivarium::VivariumError;
use vivarium::cli::{Cli, Command};
use vivarium::config::{AccountsFile, Config};
use vivarium::message;
use vivarium::store::MailStore;

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

    let config_path = cli.config.unwrap_or_else(Config::default_path);
    let config = Config::load(&config_path)?;
    let accounts_file = AccountsFile::load(&AccountsFile::default_path())?;

    // Resolve which account to use for commands that need one
    let resolve_account = |name: Option<String>| -> Result<_, VivariumError> {
        match name {
            Some(n) => Ok(accounts_file.find_account(&n)?.clone()),
            None => {
                let first = accounts_file
                    .accounts
                    .first()
                    .ok_or_else(|| VivariumError::Config("no accounts configured".into()))?;
                Ok(first.clone())
            }
        }
    };

    match cli.command {
        Command::Init => unreachable!(),
        Command::Sync { account } => {
            let account_name = account.or(cli.account);
            match account_name {
                Some(name) => {
                    let acct = accounts_file.find_account(&name)?;
                    let result = vivarium::sync::sync_account(acct, &config).await?;
                    println!("synced {}: {} new messages", name, result.new);
                }
                None => {
                    for acct in &accounts_file.accounts {
                        let result = vivarium::sync::sync_account(acct, &config).await?;
                        println!("synced {}: {} new messages", acct.name, result.new);
                    }
                }
            }
        }
        Command::List { folder } => {
            let accounts = match cli.account {
                Some(name) => vec![accounts_file.find_account(&name)?.clone()],
                None => accounts_file.accounts.clone(),
            };
            for acct in &accounts {
                println!("# {}", acct.name);
                let store = MailStore::new(&acct.mail_path(&config));
                let entries = store.list_messages(&folder)?;
                if entries.is_empty() {
                    println!("  no messages in {folder}");
                } else {
                    for entry in &entries {
                        println!("  {entry}");
                    }
                }
                println!();
            }
        }
        Command::Show { message_ids } => {
            let acct = resolve_account(cli.account)?;
            let store = MailStore::new(&acct.mail_path(&config));
            for (i, message_id) in message_ids.iter().enumerate() {
                if i > 0 {
                    println!("\n---\n");
                }
                let data = store.read_message(message_id)?;
                let output = message::render_message(&data)?;
                println!("{output}");
            }
        }
        Command::Archive { message_ids } => {
            let acct = resolve_account(cli.account)?;
            let store = MailStore::new(&acct.mail_path(&config));
            for message_id in &message_ids {
                store.move_message(message_id, "inbox", "archive")?;
                println!("archived {message_id}");
            }
        }
        Command::Watch { account } => {
            let _account_name = account.or(cli.account);
            tracing::info!("watch command stub");
        }
        Command::Send { path } => {
            let acct = resolve_account(cli.account)?;
            let data = std::fs::read(&path)?;
            vivarium::smtp::send_raw(&acct, &data).await?;
            println!("sent {}", path.display());
        }
        Command::Reply { message_id, body } => {
            let acct = resolve_account(cli.account)?;
            let store = MailStore::new(&acct.mail_path(&config));
            let original = store.read_message(&message_id)?;
            let reply_eml = message::build_reply(&original, &body, &acct.email)?;
            vivarium::smtp::send_raw(&acct, reply_eml.as_bytes()).await?;
            println!("replied to {message_id}");
        }
        Command::Compose { to, subject } => {
            let acct = resolve_account(cli.account)?;
            let store = MailStore::new(&acct.mail_path(&config));
            let draft = format!(
                "From: {}\r\nTo: {to}\r\nSubject: {subject}\r\n\r\n",
                acct.email
            );
            let draft_id = format!("draft-{}", chrono::Utc::now().timestamp());
            let path = store.store_message("drafts", &draft_id, draft.as_bytes())?;
            println!("draft created: {}", path.display());
            println!(
                "edit the file, then send with: vivarium send {}",
                path.display()
            );
        }
    }

    Ok(())
}

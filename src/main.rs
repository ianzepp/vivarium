use std::path::{Path, PathBuf};
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
    let accounts_file =
        AccountsFile::load_with_options(&AccountsFile::default_path(), cli.ignore_permissions)?;
    if cli.insecure {
        tracing::warn!("accepting invalid TLS certificates because --insecure was provided");
    }

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
        Command::Auth {
            account,
            client_id,
            client_secret,
        } => {
            let acct = resolve_account(account.or(cli.account))?;
            let client = vivarium::oauth::oauth_client(&acct, client_id, client_secret)?;
            vivarium::oauth::authorize(&acct, client).await?;
        }
        Command::Token { account } => {
            let acct = resolve_account(account.or(cli.account))?;
            let client = vivarium::oauth::oauth_client(&acct, None, None)?;
            vivarium::oauth::print_access_token(&acct, client).await?;
        }
        Command::Sync { account } => {
            let account_name = account.or(cli.account);
            match account_name {
                Some(name) => {
                    let acct = accounts_file.find_account(&name)?;
                    let result = vivarium::sync::sync_account(acct, &config, cli.insecure).await?;
                    println!("synced {}: {} new messages", name, result.new);
                }
                None => {
                    for acct in &accounts_file.accounts {
                        let result =
                            vivarium::sync::sync_account(acct, &config, cli.insecure).await?;
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
            let account_name = account.or(cli.account);
            match account_name {
                Some(name) => {
                    let acct = accounts_file.find_account(&name)?;
                    vivarium::watch::watch_account(acct, &config, cli.insecure).await?;
                }
                None => {
                    vivarium::watch::watch_all(&accounts_file.accounts, &config, cli.insecure)
                        .await?;
                }
            }
        }
        Command::Send { path } => {
            let acct = resolve_account(cli.account)?;
            let data = std::fs::read(&path)?;
            let reject_invalid_certs = acct.reject_invalid_certs(&config) && !cli.insecure;
            vivarium::smtp::send_raw(&acct, &data, reject_invalid_certs).await?;
            println!("sent {}", path.display());
        }
        Command::Reply { message_id, body } => {
            let acct = resolve_account(cli.account)?;
            let store = MailStore::new(&acct.mail_path(&config));
            let original = store.read_message(&message_id)?;
            let reply_eml = match body {
                Some(body) => message::build_reply(&original, &body, &acct.email)?,
                None => {
                    let template = message::build_reply_template(&original, &acct.email)?;
                    let Some(edited) = edit_message("reply", template.as_bytes())? else {
                        println!("reply cancelled");
                        return Ok(());
                    };
                    message::validate_message_headers(&edited)?;
                    String::from_utf8(edited).map_err(|e| {
                        VivariumError::Message(format!("edited reply is not UTF-8: {e}"))
                    })?
                }
            };
            let reject_invalid_certs = acct.reject_invalid_certs(&config) && !cli.insecure;
            vivarium::smtp::send_raw(&acct, reply_eml.as_bytes(), reject_invalid_certs).await?;
            println!("replied to {message_id}");
        }
        Command::Compose { to, subject } => {
            let acct = resolve_account(cli.account)?;
            let store = MailStore::new(&acct.mail_path(&config));
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
        }
    }

    Ok(())
}

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

fn editor_temp_path(prefix: &str) -> PathBuf {
    let unique = format!(
        "vivarium-{prefix}-{}-{}.eml",
        process::id(),
        chrono::Utc::now().timestamp_nanos_opt().unwrap_or_default()
    );
    std::env::temp_dir().join(Path::new(&unique))
}

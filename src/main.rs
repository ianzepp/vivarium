use std::process;

use clap::Parser;
use tracing_subscriber::EnvFilter;

use vivarium::cli::{Cli, Command};
use vivarium::config::Config;
use vivarium::VivariumError;

#[tokio::main]
async fn main() {
    let cli = Cli::parse();

    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| {
                if cli.verbose {
                    EnvFilter::new("vivarium=debug")
                } else {
                    EnvFilter::new("vivarium=info")
                }
            }),
        )
        .init();

    if let Err(e) = run(cli).await {
        eprintln!("error: {e}");
        process::exit(1);
    }
}

async fn run(cli: Cli) -> Result<(), VivariumError> {
    let config_path = cli.config.unwrap_or_else(Config::default_path);
    tracing::debug!(path = %config_path.display(), "loading config");

    match cli.command {
        Command::Sync { account } => {
            let _account_name = account.or(cli.account);
            tracing::info!("sync command stub");
        }
        Command::Watch { account } => {
            let _account_name = account.or(cli.account);
            tracing::info!("watch command stub");
        }
        Command::Send { path } => {
            tracing::info!(path = %path.display(), "send command stub");
        }
        Command::List { folder } => {
            tracing::info!(folder, "list command stub");
        }
        Command::Show { message_id } => {
            tracing::info!(message_id, "show command stub");
        }
        Command::Reply { message_id } => {
            tracing::info!(message_id, "reply command stub");
        }
        Command::Compose { to, subject } => {
            tracing::info!(to, subject, "compose command stub");
        }
        Command::Archive { message_id } => {
            tracing::info!(message_id, "archive command stub");
        }
    }

    Ok(())
}

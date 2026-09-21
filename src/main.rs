#![deny(clippy::pedantic)]

//! The `vivi` binary.
//!
//! Project-mailspace commands are handled here. Email commands are routed into
//! the `vivi-mail` crate, which owns their runtime and dispatch.

use std::process;

use clap::Parser;
use tracing_subscriber::EnvFilter;

use vivarium::VivariumError;
use vivarium::cli::{Cli, Command};
use vivi_mail::Runtime;
use vivi_mail::cli::MailCommand;

mod local_board_command;
mod local_boot_command;
mod local_goal_command;
mod local_graph_command;
mod local_mail_list;
mod local_mailspace_command;
mod local_mailspace_dump;
mod local_role_command;
mod local_step_command;
mod local_work_command;
mod local_work_list;

use local_mailspace_command::run_mailspace_command;

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
    let Cli {
        config,
        account,
        insecure,
        ignore_permissions,
        command,
        ..
    } = cli;

    // Bootstrap writes config.toml and accounts.toml, so it must run before the
    // mail runtime tries to load them.
    if matches!(command, Command::Init) {
        return vivi_mail::init::run_init();
    }
    if run_mailspace_command(&command)? {
        return Ok(());
    }

    let mail = mail_command(command);
    let runtime = Runtime::load(config, account, insecure, ignore_permissions)?;
    runtime.run(mail).await
}

/// Map a unified command onto the mail crate's own command set.
///
/// Project-mailspace variants never reach here: `run_mailspace_command`
/// consumes them first.
fn mail_command(command: Command) -> MailCommand {
    match command {
        Command::Init => MailCommand::Init,

        #[cfg(feature = "outbox")]
        Command::Auth(args) => MailCommand::Auth(args),
        #[cfg(feature = "outbox")]
        Command::Token(args) => MailCommand::Token(args),

        Command::Sync(args) => MailCommand::Sync(args),
        Command::SyncEvents(args) => MailCommand::SyncEvents(args),
        Command::Folders(args) => MailCommand::Folders(args),
        Command::Doctor(args) => MailCommand::Doctor(args),
        Command::Proton(args) => MailCommand::Proton(args),
        Command::Render(command) => MailCommand::Render(command),
        Command::WatchInbox(args) => MailCommand::WatchInbox(args),
        Command::List(args) => MailCommand::List(args),
        Command::Show(args) => MailCommand::Show(args),
        Command::Thread(args) => MailCommand::Thread(args),
        Command::Reply(command) => MailCommand::Reply(command),
        Command::Compose(command) => MailCommand::Compose(command),
        Command::Export(args) => MailCommand::Export(args),
        Command::Search(args) => MailCommand::Search(args),
        Command::Index(args) => MailCommand::Index(args),
        Command::Agent(args) => MailCommand::Agent(args),
        Command::Exec(args) => MailCommand::Exec(args),
        Command::Enqueue(args) => MailCommand::Enqueue(args),
        Command::Queue(args) => MailCommand::Queue(args),
        Command::Labels(args) => MailCommand::Labels(args),
        Command::Label(args) => MailCommand::Label(args),

        Command::Board(_)
        | Command::Boot { .. }
        | Command::Mailspace { .. }
        | Command::Mail { .. }
        | Command::Task { .. }
        | Command::Need { .. }
        | Command::Want { .. }
        | Command::Memo { .. }
        | Command::Goal { .. }
        | Command::Role { .. }
        | Command::Cycle { .. }
        | Command::Trace(_)
        | Command::Graph { .. }
        | Command::Step { .. } => unreachable!(),
    }
}

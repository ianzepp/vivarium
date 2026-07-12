use anyhow::Result;
use clap::{Parser, Subcommand};
use serde_json::{Value, json};
use std::env;
use std::path::Path;
use std::path::PathBuf;
use vivarium::mailspace::Mailspace;
use vivi_pty::{client, daemon::Daemon};

#[derive(Parser)]
#[command(version, about)]
struct Cli {
    /// Project root that owns the Vivi mailspace and PTY runtime
    #[arg(long, global = true)]
    project: Option<PathBuf>,

    #[arg(long, global = true)]
    socket: Option<PathBuf>,

    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    Daemon,
    Info,
    Session {
        #[command(subcommand)]
        command: SessionCommand,
    },
    Terminal {
        #[command(subcommand)]
        command: TerminalCommand,
    },
}

#[derive(Subcommand)]
enum SessionCommand {
    List,
    Start {
        session_id: String,
        #[arg(long, default_value = "generic")]
        driver: String,
        #[arg(long)]
        cwd: PathBuf,
        #[arg(long, default_value_t = 120)]
        columns: u16,
        #[arg(long, default_value_t = 40)]
        rows: u16,
        #[arg(required = true, trailing_var_arg = true)]
        command: Vec<String>,
    },
    Inspect {
        session_id: String,
    },
    Stop {
        session_id: String,
    },
}

#[derive(Subcommand)]
enum TerminalCommand {
    Write {
        session_id: String,
        data: String,
        #[arg(long)]
        enter: bool,
    },
    Snapshot {
        session_id: String,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let socket = match cli.socket {
        Some(socket) => socket,
        None => default_socket_path(cli.project.as_deref())?,
    };

    match cli.command {
        Command::Daemon => Daemon::new(socket).run(),
        Command::Info => print_result(client::call(&socket, "daemon.info", json!({}))?),
        Command::Session { command } => run_session(&socket, command),
        Command::Terminal { command } => run_terminal(&socket, command),
    }
}

fn run_terminal(socket: &std::path::Path, command: TerminalCommand) -> Result<()> {
    let result = match command {
        TerminalCommand::Write {
            session_id,
            mut data,
            enter,
        } => {
            if enter {
                data.push('\r');
            }
            client::call(
                socket,
                "terminal.write",
                json!({ "session_id": session_id, "data": data }),
            )?
        }
        TerminalCommand::Snapshot { session_id } => client::call(
            socket,
            "terminal.snapshot",
            json!({ "session_id": session_id }),
        )?,
    };
    print_result(result)
}

fn run_session(socket: &std::path::Path, command: SessionCommand) -> Result<()> {
    let result = match command {
        SessionCommand::List => client::call(socket, "session.list", json!({}))?,
        SessionCommand::Start {
            session_id,
            driver,
            cwd,
            columns,
            rows,
            command,
        } => client::call(
            socket,
            "session.start",
            json!({
                "session_id": session_id,
                "driver": driver,
                "command": command,
                "cwd": cwd,
                "columns": columns,
                "rows": rows,
            }),
        )?,
        SessionCommand::Inspect { session_id } => client::call(
            socket,
            "session.inspect",
            json!({ "session_id": session_id }),
        )?,
        SessionCommand::Stop { session_id } => {
            client::call(socket, "session.stop", json!({ "session_id": session_id }))?
        }
    };
    print_result(result)
}

fn print_result(result: Value) -> Result<()> {
    println!("{}", serde_json::to_string_pretty(&result)?);
    Ok(())
}

fn default_socket_path(project: Option<&Path>) -> Result<PathBuf> {
    if let Some(socket) = env::var_os("VIVI_PTY_SOCKET") {
        return Ok(PathBuf::from(socket));
    }
    let mailspace = Mailspace::discover(project)?;
    Ok(mailspace.dir.join("vivi-pty.sock"))
}

#[cfg(test)]
#[path = "main_test.rs"]
mod tests;

use anyhow::{Result, bail};
use clap::{Parser, Subcommand};
use serde_json::{Value, json};
use std::env;
use std::path::Path;
use std::path::PathBuf;
use vivarium::mailspace::Mailspace;
use vivi_pty::{client, daemon::Daemon, protocol::KeyModifier};

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
    /// Stop and drop the session id (no tombstone) so a new start can rebind
    /// command/cwd for the same session_id.
    Remove {
        session_id: String,
    },
    Restart {
        session_id: String,
    },
    Diagnostic {
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
    WriteBytes {
        session_id: String,
        /// Raw input bytes encoded as hexadecimal.
        data: String,
    },
    Key {
        session_id: String,
        key: String,
        #[arg(long, value_delimiter = ',')]
        modifiers: Vec<String>,
    },
    Resize {
        session_id: String,
        columns: u16,
        rows: u16,
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
        TerminalCommand::WriteBytes { session_id, data } => client::call(
            socket,
            "terminal.write_bytes",
            json!({ "session_id": session_id, "data": decode_hex(&data)? }),
        )?,
        TerminalCommand::Key {
            session_id,
            key,
            modifiers,
        } => {
            let modifiers = modifiers
                .iter()
                .map(|modifier| parse_modifier(modifier))
                .collect::<Result<Vec<_>>>()?;
            client::call(
                socket,
                "terminal.key",
                json!({ "session_id": session_id, "key": key, "modifiers": modifiers }),
            )?
        }
        TerminalCommand::Resize {
            session_id,
            columns,
            rows,
        } => client::call(
            socket,
            "terminal.resize",
            json!({ "session_id": session_id, "columns": columns, "rows": rows }),
        )?,
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
        SessionCommand::Remove { session_id } => client::call(
            socket,
            "session.remove",
            json!({ "session_id": session_id }),
        )?,
        SessionCommand::Restart { session_id } => {
            let operation_id = format!(
                "restart-{}",
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_nanos()
            );
            client::call_with_operation_id(
                socket,
                "session.restart",
                json!({ "session_id": session_id }),
                operation_id,
            )?
        }
        SessionCommand::Diagnostic { session_id } => client::call(
            socket,
            "session.diagnostic",
            json!({ "session_id": session_id }),
        )?,
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

fn parse_modifier(modifier: &str) -> Result<KeyModifier> {
    match modifier.to_ascii_lowercase().as_str() {
        "control" | "ctrl" => Ok(KeyModifier::Control),
        "alt" | "meta" => Ok(KeyModifier::Alt),
        "shift" => Ok(KeyModifier::Shift),
        _ => bail!("unsupported key modifier: {modifier}"),
    }
}

fn decode_hex(input: &str) -> Result<Vec<u8>> {
    if !input.len().is_multiple_of(2) {
        bail!("raw byte input must contain an even number of hexadecimal digits");
    }
    input
        .as_bytes()
        .chunks_exact(2)
        .map(|pair| {
            let high = hex_digit(pair[0])?;
            let low = hex_digit(pair[1])?;
            Ok((high << 4) | low)
        })
        .collect()
}

fn hex_digit(byte: u8) -> Result<u8> {
    match byte {
        b'0'..=b'9' => Ok(byte - b'0'),
        b'a'..=b'f' => Ok(byte - b'a' + 10),
        b'A'..=b'F' => Ok(byte - b'A' + 10),
        _ => bail!("invalid hexadecimal digit: {:?}", char::from(byte)),
    }
}

#[cfg(test)]
#[path = "main_test.rs"]
mod tests;

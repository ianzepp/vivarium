use crate::protocol::{
    DaemonInfo, PROTOCOL_VERSION, Request, Response, error_codes, read_frame, write_frame,
};
use anyhow::{Context, Result, bail};
use serde::de::DeserializeOwned;
use serde_json::{Value, json};
use signal_hook::consts::{SIGINT, SIGTERM};
use signal_hook::flag as signal_flag;
use std::fs;
use std::io;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use std::thread;
use std::time::Duration;

#[path = "daemon/registry.rs"]
mod registry;
#[path = "session.rs"]
mod session;

#[cfg(test)]
use crate::protocol::{
    DiagnosticSnapshot, SessionSelector, SessionState, StartSession, TerminalResize, TerminalWrite,
    TerminalWriteBytes,
};
use registry::{SessionError, SessionRegistry};
#[cfg(test)]
use session::process_group_exists;

const MAX_SESSIONS: usize = 64;
const MAX_TOMBSTONES: usize = 32;
const MAX_SESSION_ID_BYTES: usize = 128;
const MAX_COMMAND_ARGS: usize = 128;
const MAX_COMMAND_BYTES: usize = 64 * 1024;
const MAX_COLUMNS: u16 = 500;
const MAX_ROWS: u16 = 200;
const MAX_SCROLLBACK_ROWS: usize = 2_000;
const TERMINATION_GRACE: Duration = Duration::from_millis(500);
const TERMINATION_SETTLE: Duration = Duration::from_millis(100);
const POLL_INTERVAL: Duration = Duration::from_millis(10);

pub struct Daemon {
    socket_path: PathBuf,
    sessions: Arc<SessionRegistry>,
}

impl Daemon {
    pub fn new(socket_path: PathBuf) -> Self {
        Self {
            socket_path,
            sessions: Arc::new(SessionRegistry::default()),
        }
    }

    pub fn run(self) -> Result<()> {
        prepare_socket(&self.socket_path)?;
        let listener = UnixListener::bind(&self.socket_path)
            .with_context(|| format!("bind {}", self.socket_path.display()))?;
        listener
            .set_nonblocking(true)
            .context("configure daemon listener")?;

        let shutdown = Arc::new(AtomicBool::new(false));
        signal_flag::register(SIGINT, Arc::clone(&shutdown)).context("register SIGINT handler")?;
        signal_flag::register(SIGTERM, Arc::clone(&shutdown))
            .context("register SIGTERM handler")?;

        while !shutdown.load(Ordering::Relaxed) {
            match listener.accept() {
                Ok((stream, _)) => {
                    let sessions = Arc::clone(&self.sessions);
                    thread::spawn(move || {
                        if let Err(error) = serve_client(stream, sessions) {
                            eprintln!("client connection failed: {error:#}");
                        }
                    });
                }
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                    thread::sleep(POLL_INTERVAL);
                }
                Err(error) => {
                    eprintln!("accept failed: {error}");
                    thread::sleep(POLL_INTERVAL);
                }
            }
        }

        self.sessions.shutdown();
        Ok(())
    }
}

impl Drop for Daemon {
    fn drop(&mut self) {
        self.sessions.shutdown();
        let _ = fs::remove_file(&self.socket_path);
    }
}

fn prepare_socket(path: &Path) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("create socket directory {}", parent.display()))?;
    }
    if path.exists() {
        match UnixStream::connect(path) {
            Ok(_) => bail!("daemon already listening at {}", path.display()),
            Err(_) => fs::remove_file(path)
                .with_context(|| format!("remove stale socket {}", path.display()))?,
        }
    }
    Ok(())
}

fn serve_client(mut stream: UnixStream, sessions: Arc<SessionRegistry>) -> Result<()> {
    loop {
        let request: Request = match read_frame(&mut stream) {
            Ok(request) => request,
            Err(error) if error.kind() == io::ErrorKind::UnexpectedEof => return Ok(()),
            Err(error) => return Err(error.into()),
        };
        let response = dispatch(request, &sessions);
        write_frame(&mut stream, &response)?;
    }
}

fn dispatch(request: Request, sessions: &SessionRegistry) -> Response {
    if request.jsonrpc != "2.0" {
        return Response::error(
            request.id,
            error_codes::INVALID_REQUEST,
            "jsonrpc must be 2.0",
        );
    }
    if sessions.is_shutting_down() && request.method != "daemon.info" {
        return Response::error(
            request.id,
            error_codes::INVALID_STATE,
            "daemon is shutting down",
        );
    }

    let result: std::result::Result<Value, DispatchError> = match request.method.as_str() {
        "daemon.info" => Ok(json!(DaemonInfo {
            name: "vivi-ptyd".into(),
            version: env!("CARGO_PKG_VERSION").into(),
            protocol_version: PROTOCOL_VERSION,
        })),
        "session.list" => sessions
            .list()
            .map(|sessions| json!({ "sessions": sessions }))
            .map_err(Into::into),
        "session.start" => parse(request.params)
            .and_then(|params| sessions.start(params).map_err(Into::into))
            .map(|session| json!(session)),
        "session.inspect" => parse(request.params)
            .and_then(|params| sessions.inspect(params).map_err(Into::into))
            .map(|session| json!(session)),
        "session.stop" => parse(request.params)
            .and_then(|params| sessions.stop(params).map_err(Into::into))
            .map(|session| json!(session)),
        "session.diagnostic" => parse(request.params)
            .and_then(|params| sessions.diagnostic(params).map_err(Into::into))
            .map(|snapshot| json!(snapshot)),
        "terminal.write" => parse(request.params)
            .and_then(|params| sessions.write(params).map_err(Into::into))
            .map(|written| json!({ "written": written })),
        "terminal.write_bytes" => parse(request.params)
            .and_then(|params| sessions.write_bytes(params).map_err(Into::into))
            .map(|written| json!({ "written": written })),
        "terminal.key" => parse(request.params)
            .and_then(|params| sessions.key(params).map_err(Into::into))
            .map(|written| json!({ "written": written })),
        "terminal.resize" => parse(request.params)
            .and_then(|params| sessions.resize(params).map_err(Into::into))
            .map(|snapshot| json!(snapshot)),
        "terminal.snapshot" => parse(request.params)
            .and_then(|params| sessions.snapshot(params).map_err(Into::into))
            .map(|snapshot| json!(snapshot)),
        _ => {
            return Response::error(
                request.id,
                error_codes::METHOD_NOT_FOUND,
                "method not found",
            );
        }
    };

    match result {
        Ok(value) => Response::success(request.id, value),
        Err(error) => Response::error(request.id, error.code, error.message),
    }
}

#[derive(Debug)]
struct DispatchError {
    code: i32,
    message: String,
}

impl DispatchError {
    fn invalid_params(error: impl std::fmt::Display) -> Self {
        Self {
            code: error_codes::INVALID_PARAMS,
            message: format!("invalid method parameters: {error}"),
        }
    }
}

impl From<SessionError> for DispatchError {
    fn from(error: SessionError) -> Self {
        let code = match error {
            SessionError::NotFound(_) => error_codes::SESSION_NOT_FOUND,
            SessionError::Conflict(_) => error_codes::SESSION_CONFLICT,
            SessionError::InvalidState(_) => error_codes::INVALID_STATE,
            SessionError::ResourceLimit(_) => error_codes::RESOURCE_LIMIT,
            SessionError::InvalidInput(_) => error_codes::INVALID_PARAMS,
            SessionError::Internal(_) => error_codes::INTERNAL,
        };
        Self {
            code,
            message: error.to_string(),
        }
    }
}

fn parse<T: DeserializeOwned>(value: Value) -> std::result::Result<T, DispatchError> {
    serde_json::from_value(value).map_err(DispatchError::invalid_params)
}

#[cfg(test)]
#[path = "daemon_test.rs"]
mod tests;

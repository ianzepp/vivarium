#[cfg(test)]
use crate::protocol::SessionState;
use crate::protocol::{
    DaemonInfo, PROTOCOL_VERSION, Request, Response, SessionInfo, SessionSelector, StartSession,
    TerminalSnapshot, TerminalWrite, error_codes, read_frame, write_frame,
};
use anyhow::{Context, Result, bail};
use serde::de::DeserializeOwned;
use serde_json::{Value, json};
use signal_hook::consts::{SIGINT, SIGTERM};
use signal_hook::flag as signal_flag;
use std::collections::{HashMap, VecDeque};
use std::fs;
use std::io;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};
use std::thread;
use std::time::Duration;

#[path = "session.rs"]
mod session;

use session::ManagedSession;
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
        "terminal.write" => parse(request.params)
            .and_then(|params| sessions.write(params).map_err(Into::into))
            .map(|written| json!({ "written": written })),
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

#[derive(Debug)]
enum SessionError {
    NotFound(String),
    Conflict(String),
    InvalidState(String),
    ResourceLimit(String),
    InvalidInput(String),
    Internal(anyhow::Error),
}

impl std::fmt::Display for SessionError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotFound(message)
            | Self::Conflict(message)
            | Self::InvalidState(message)
            | Self::ResourceLimit(message)
            | Self::InvalidInput(message) => formatter.write_str(message),
            Self::Internal(error) => write!(formatter, "{error:#}"),
        }
    }
}

impl From<anyhow::Error> for SessionError {
    fn from(error: anyhow::Error) -> Self {
        Self::Internal(error)
    }
}

#[derive(Default)]
struct RegistryState {
    sessions: HashMap<String, ManagedSession>,
    tombstones: VecDeque<String>,
}

#[derive(Default)]
struct SessionRegistry {
    state: Mutex<RegistryState>,
    shutting_down: AtomicBool,
}

impl SessionRegistry {
    fn is_shutting_down(&self) -> bool {
        self.shutting_down.load(Ordering::Acquire)
    }

    fn list(&self) -> std::result::Result<Vec<SessionInfo>, SessionError> {
        let mut state = self.state.lock().expect("session registry poisoned");
        refresh_all(&mut state)?;
        let mut result = state
            .sessions
            .values()
            .map(|session| session.info.clone())
            .collect::<Vec<_>>();
        result.sort_by(|left, right| left.session_id.cmp(&right.session_id));
        Ok(result)
    }

    fn start(&self, request: StartSession) -> std::result::Result<SessionInfo, SessionError> {
        validate_start(&request).map_err(SessionError::InvalidInput)?;
        let mut state = self.state.lock().expect("session registry poisoned");
        if self.is_shutting_down() {
            return Err(SessionError::InvalidState("daemon is shutting down".into()));
        }
        refresh_all(&mut state)?;
        if state.sessions.contains_key(&request.session_id) {
            return Err(SessionError::Conflict(format!(
                "session already exists: {}",
                request.session_id
            )));
        }
        if state.sessions.len() >= MAX_SESSIONS {
            evict_oldest_tombstone(&mut state).ok_or_else(|| {
                SessionError::ResourceLimit(format!(
                    "maximum session count reached: {MAX_SESSIONS}"
                ))
            })?;
        }

        let session = ManagedSession::spawn(request)?;
        let info = session.info.clone();
        state.sessions.insert(info.session_id.clone(), session);
        Ok(info)
    }

    fn inspect(&self, selector: SessionSelector) -> std::result::Result<SessionInfo, SessionError> {
        let mut state = self.state.lock().expect("session registry poisoned");
        let (info, transitioned) = {
            let session = state
                .sessions
                .get_mut(&selector.session_id)
                .ok_or_else(|| {
                    SessionError::NotFound(format!("unknown session: {}", selector.session_id))
                })?;
            let transitioned = session.refresh()?;
            (session.info.clone(), transitioned)
        };
        if transitioned {
            record_tombstone(&mut state, selector.session_id);
        }
        Ok(info)
    }

    fn stop(&self, selector: SessionSelector) -> std::result::Result<SessionInfo, SessionError> {
        let mut state = self.state.lock().expect("session registry poisoned");
        let (info, transitioned) = {
            let session = state
                .sessions
                .get_mut(&selector.session_id)
                .ok_or_else(|| {
                    SessionError::NotFound(format!("unknown session: {}", selector.session_id))
                })?;
            let transitioned = session.stop()?;
            (session.info.clone(), transitioned)
        };
        if transitioned {
            record_tombstone(&mut state, selector.session_id);
        }
        Ok(info)
    }

    fn write(&self, request: TerminalWrite) -> std::result::Result<usize, SessionError> {
        let mut state = self.state.lock().expect("session registry poisoned");
        let (written, transitioned) = {
            let session = state.sessions.get_mut(&request.session_id).ok_or_else(|| {
                SessionError::NotFound(format!("unknown session: {}", request.session_id))
            })?;
            let transitioned = session.refresh()?;
            let written = session.write_running(request.data.as_bytes())?;
            (written, transitioned)
        };
        if transitioned {
            record_tombstone(&mut state, request.session_id);
        }
        Ok(written)
    }

    fn snapshot(
        &self,
        selector: SessionSelector,
    ) -> std::result::Result<TerminalSnapshot, SessionError> {
        let state = self.state.lock().expect("session registry poisoned");
        let session = state.sessions.get(&selector.session_id).ok_or_else(|| {
            SessionError::NotFound(format!("unknown session: {}", selector.session_id))
        })?;
        Ok(session.snapshot())
    }

    fn shutdown(&self) {
        self.shutting_down.store(true, Ordering::Release);
        let mut state = self.state.lock().expect("session registry poisoned");
        for session in state.sessions.values_mut() {
            if let Err(error) = session.stop() {
                eprintln!(
                    "failed to stop session {}: {error:#}",
                    session.info.session_id
                );
                session.kill_group_best_effort();
            }
        }
    }
}

impl Drop for SessionRegistry {
    fn drop(&mut self) {
        self.shutting_down.store(true, Ordering::Release);
        let mut state = self.state.lock().expect("session registry poisoned");
        for session in state.sessions.values_mut() {
            if let Err(error) = session.stop() {
                eprintln!(
                    "failed to stop session {}: {error:#}",
                    session.info.session_id
                );
                session.kill_group_best_effort();
            }
        }
    }
}

fn refresh_all(state: &mut RegistryState) -> std::result::Result<(), SessionError> {
    let ids = state.sessions.keys().cloned().collect::<Vec<_>>();
    for session_id in ids {
        let Some(session) = state.sessions.get_mut(&session_id) else {
            continue;
        };
        let transitioned = session.refresh()?;
        if transitioned {
            record_tombstone(state, session_id);
        }
    }
    trim_tombstones(state);
    Ok(())
}

fn record_tombstone(state: &mut RegistryState, session_id: String) {
    if !state.tombstones.contains(&session_id) {
        state.tombstones.push_back(session_id);
    }
    trim_tombstones(state);
}

fn trim_tombstones(state: &mut RegistryState) {
    while state.tombstones.len() > MAX_TOMBSTONES {
        let Some(session_id) = state.tombstones.pop_front() else {
            break;
        };
        state.sessions.remove(&session_id);
    }
}

fn evict_oldest_tombstone(state: &mut RegistryState) -> Option<()> {
    while let Some(session_id) = state.tombstones.pop_front() {
        if state.sessions.remove(&session_id).is_some() {
            return Some(());
        }
    }
    None
}

fn validate_start(request: &StartSession) -> std::result::Result<(), String> {
    if request.session_id.is_empty() || request.session_id.len() > MAX_SESSION_ID_BYTES {
        return Err(format!(
            "session_id must be 1..={MAX_SESSION_ID_BYTES} bytes"
        ));
    }
    if !request
        .session_id
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
    {
        return Err("session_id contains an unsupported character".into());
    }
    if request.command.is_empty() || request.command[0].trim().is_empty() {
        return Err("command cannot be empty".into());
    }
    if request.command.len() > MAX_COMMAND_ARGS {
        return Err(format!(
            "command has too many arguments: {MAX_COMMAND_ARGS} maximum"
        ));
    }
    let command_bytes = request
        .command
        .iter()
        .map(|argument| argument.len())
        .sum::<usize>();
    if command_bytes > MAX_COMMAND_BYTES {
        return Err(format!(
            "command arguments exceed {MAX_COMMAND_BYTES} bytes"
        ));
    }
    if !Path::new(&request.cwd).is_dir() {
        return Err(format!("cwd is not a directory: {}", request.cwd));
    }
    if request.columns == 0
        || request.rows == 0
        || request.columns > MAX_COLUMNS
        || request.rows > MAX_ROWS
    {
        return Err(format!(
            "terminal dimensions must be within 1..={MAX_COLUMNS} columns and 1..={MAX_ROWS} rows"
        ));
    }
    Ok(())
}

#[cfg(test)]
#[path = "daemon_test.rs"]
mod tests;

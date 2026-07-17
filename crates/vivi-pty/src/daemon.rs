use crate::lease::LeaseError;
use crate::protocol::{
    AttachmentAck, DaemonInfo, PROTOCOL_VERSION, Request, Response, ServerNotification,
    SessionSelector, SubscriptionAck, error_codes, read_frame, write_frame,
};
use anyhow::{Context, Result, bail};
use libc;
use serde::de::DeserializeOwned;
use serde_json::{Value, json};
use signal_hook::consts::{SIGINT, SIGTERM};
use signal_hook::flag as signal_flag;
use std::fs;
use std::io;
use std::os::unix::fs::FileTypeExt;
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
use crate::events::MAX_EVENT_HISTORY;
#[cfg(test)]
use crate::protocol::{
    ControlLease, DiagnosticSnapshot, EventBatch, LeasedTerminalWrite, SemanticOutcome,
    SessionEventKind, SessionLeaseRelease, SessionState, SessionWait, StartSession, TerminalResize,
    TerminalWrite, TerminalWriteBytes,
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
    if !path.exists() {
        return Ok(());
    }
    let metadata = fs::metadata(path).with_context(|| format!("stat socket {}", path.display()))?;
    if !metadata.file_type().is_socket() {
        fs::remove_file(path)
            .with_context(|| format!("remove non-socket file {}", path.display()))?;
        return Ok(());
    }
    // A live daemon will accept the connection. ECONNREFUSED means the socket
    // file is stale. Retry briefly to avoid racing a daemon that is still
    // binding its listener.
    for _ in 0..3 {
        match UnixStream::connect(path) {
            Ok(_) => bail!("daemon already listening at {}", path.display()),
            Err(error) if error.raw_os_error() == Some(libc::ECONNREFUSED) => {
                std::thread::sleep(Duration::from_millis(10));
            }
            Err(error) => {
                return Err(error).with_context(|| {
                    format!(
                        "socket at {} is not reachable; not removing",
                        path.display()
                    )
                });
            }
        }
    }
    fs::remove_file(path).with_context(|| format!("remove stale socket {}", path.display()))?;
    Ok(())
}

struct ActiveSubscription {
    session_id: String,
    next_sequence: u64,
}

fn serve_client(mut stream: UnixStream, sessions: Arc<SessionRegistry>) -> Result<()> {
    stream
        .set_nonblocking(false)
        .context("configure client socket blocking")?;
    stream
        .set_read_timeout(Some(Duration::from_millis(50)))
        .context("configure client read timeout")?;
    let mut subscription: Option<ActiveSubscription> = None;
    loop {
        if let Some(active) = subscription.as_mut() {
            let selector = SessionSelector {
                session_id: active.session_id.clone(),
            };
            let batch = match sessions.event_batch(selector, active.next_sequence) {
                Ok(batch) => batch,
                Err(SessionError::NotFound(_)) => {
                    subscription = None;
                    continue;
                }
                Err(error) => return Err(anyhow::anyhow!(error.to_string())),
            };
            if batch.lagged || !batch.events.is_empty() {
                let latest_sequence = batch.latest_sequence;
                let notification = ServerNotification::event(batch).map_err(io::Error::other)?;
                write_frame(&mut stream, &notification)?;
                active.next_sequence = latest_sequence;
            }
        }
        let request: Request = match read_frame(&mut stream) {
            Ok(request) => request,
            Err(error) if error.kind() == io::ErrorKind::UnexpectedEof => return Ok(()),
            Err(error)
                if matches!(
                    error.kind(),
                    io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut
                ) =>
            {
                continue;
            }
            Err(error) => return Err(error.into()),
        };
        let method = request.method.clone();
        let response = dispatch(request, &sessions);
        if method == "session.subscribe" && response.error.is_none() {
            if let Some(result) = response.result.as_ref()
                && let Ok(ack) = serde_json::from_value::<SubscriptionAck>(result.clone())
            {
                subscription = Some(ActiveSubscription {
                    session_id: ack.session_id,
                    next_sequence: ack.next_sequence,
                });
            }
        } else if method == "session.attach" && response.error.is_none() {
            if let Some(result) = response.result.as_ref()
                && let Ok(ack) = serde_json::from_value::<AttachmentAck>(result.clone())
            {
                subscription = Some(ActiveSubscription {
                    session_id: ack.session_id,
                    next_sequence: ack.next_sequence,
                });
            }
        } else if method == "session.unsubscribe" && response.error.is_none() {
            subscription = None;
        }
        write_frame(&mut stream, &response)?;
    }
}

fn dispatch(request: Request, sessions: &SessionRegistry) -> Response {
    let method = request.method.clone();
    let params = request.params.clone();
    let operation_id = request.operation_id.clone();
    let session_id = request
        .params
        .get("session_id")
        .and_then(Value::as_str)
        .map(str::to_owned);
    if let Some(operation_id) = operation_id.as_deref() {
        let fingerprint = json!({ "method": method, "params": params });
        let request_id = request.id.clone();
        return sessions.with_operation(
            operation_id,
            &fingerprint,
            request_id,
            session_id.as_deref(),
            &method,
            || dispatch_request(request, sessions),
        );
    }
    dispatch_request(request, sessions)
}

fn dispatch_request(request: Request, sessions: &SessionRegistry) -> Response {
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
        "daemon.capabilities" => Ok(json!(crate::mcp::McpBridge::default().capabilities())),
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
        "session.remove" => parse(request.params)
            .and_then(|params| sessions.remove(params).map_err(Into::into))
            .map(|session| json!(session)),
        "session.diagnostic" => parse(request.params)
            .and_then(|params| sessions.diagnostic(params).map_err(Into::into))
            .map(|snapshot| json!(snapshot)),
        "session.subscribe" => parse(request.params)
            .and_then(|params| sessions.subscribe(params).map_err(Into::into))
            .map(|ack| json!(ack)),
        "session.attach" => parse(request.params)
            .and_then(|params| sessions.attach(params).map_err(Into::into))
            .map(|ack| json!(ack)),
        "session.unsubscribe" => Ok(json!({ "unsubscribed": true })),
        "session.lease.acquire" => parse(request.params)
            .and_then(|params| sessions.acquire_lease(params).map_err(Into::into))
            .map(|lease| json!(lease)),
        "session.lease.release" => parse(request.params)
            .and_then(|params| sessions.release_lease(params).map_err(Into::into))
            .map(|result| json!(result)),
        "session.wait" => parse(request.params)
            .and_then(|params| sessions.wait(params).map_err(Into::into))
            .map(|snapshot| json!(snapshot)),
        "terminal.write" => parse(request.params)
            .and_then(|params| sessions.write(params).map_err(Into::into))
            .map(|written| json!({ "written": written })),
        "terminal.write_bytes" => parse(request.params)
            .and_then(|params| sessions.write_bytes(params).map_err(Into::into))
            .map(|written| json!({ "written": written })),
        "terminal.control_write" => parse(request.params)
            .and_then(|params| sessions.control_write(params).map_err(Into::into))
            .map(|written| json!({ "written": written })),
        "terminal.control_write_bytes" => parse(request.params)
            .and_then(|params| sessions.control_write_bytes(params).map_err(Into::into))
            .map(|written| json!({ "written": written })),
        "terminal.key" => parse(request.params)
            .and_then(|params| sessions.key(params).map_err(Into::into))
            .map(|written| json!({ "written": written })),
        "terminal.control_key" => parse(request.params)
            .and_then(|params| sessions.control_key(params).map_err(Into::into))
            .map(|written| json!({ "written": written })),
        "terminal.resize" => parse(request.params)
            .and_then(|params| sessions.resize(params).map_err(Into::into))
            .map(|snapshot| json!(snapshot)),
        "terminal.control_resize" => parse(request.params)
            .and_then(|params| sessions.control_resize(params).map_err(Into::into))
            .map(|snapshot| json!(snapshot)),
        "terminal.snapshot" => parse(request.params)
            .and_then(|params| sessions.snapshot(params).map_err(Into::into))
            .map(|snapshot| json!(snapshot)),
        "session.submit" => require_operation_id(&request).and_then(|operation_id| {
            parse(request.params).and_then(|params| {
                sessions
                    .submit(params, operation_id)
                    .map_err(Into::into)
                    .map(|outcome| json!(outcome))
            })
        }),
        "session.interrupt" => require_operation_id(&request).and_then(|operation_id| {
            parse(request.params).and_then(|params| {
                sessions
                    .interrupt(params, operation_id)
                    .map_err(Into::into)
                    .map(|outcome| json!(outcome))
            })
        }),
        "session.restart" => require_operation_id(&request).and_then(|operation_id| {
            parse(request.params).and_then(|params| {
                sessions
                    .restart(params, operation_id)
                    .map_err(Into::into)
                    .map(|outcome| json!(outcome))
            })
        }),
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
            SessionError::Timeout(_) => error_codes::TIMEOUT,
            SessionError::Lease(LeaseError::Busy(_)) => error_codes::LEASE_CONFLICT,
            SessionError::Lease(LeaseError::InvalidInput(_)) => error_codes::INVALID_PARAMS,
            SessionError::Lease(LeaseError::NotFound(_) | LeaseError::Expired(_)) => {
                error_codes::LEASE_REQUIRED
            }
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

fn require_operation_id(request: &Request) -> std::result::Result<String, DispatchError> {
    request.operation_id.clone().ok_or_else(|| DispatchError {
        code: error_codes::INVALID_PARAMS,
        message: format!("{} requires operation_id", request.method),
    })
}

#[cfg(test)]
#[path = "daemon_test.rs"]
mod tests;

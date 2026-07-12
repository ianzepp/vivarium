use crate::driver::DriverRegistry;
use crate::events::EventHub;
use crate::keys::encode_key;
use crate::lease::{LeaseError, LeaseManager};
use crate::operation::{OperationStore, validate_operation_id};
use crate::protocol::{
    AttachmentAck, ControlLease, DaemonInfo, DiagnosticSnapshot, EventBatch, LeasedTerminalKey,
    LeasedTerminalResize, LeasedTerminalWrite, LeasedTerminalWriteBytes, PROTOCOL_VERSION,
    SessionAttach, SessionEventKind, SessionInfo, SessionLeaseAcquire, SessionLeaseRelease,
    SessionSelector, SessionSubscribe, SessionWait, StartSession, SubscriptionAck, TerminalKey,
    TerminalResize, TerminalSnapshot, TerminalWrite, TerminalWriteBytes,
};
use anyhow::Error;
use std::collections::{HashMap, VecDeque};
use std::path::Path;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};

use super::session::ManagedSession;
use super::{
    MAX_COLUMNS, MAX_COMMAND_ARGS, MAX_COMMAND_BYTES, MAX_ROWS, MAX_SESSION_ID_BYTES, MAX_SESSIONS,
    MAX_TOMBSTONES,
};

#[path = "registry/control.rs"]
mod control;
#[path = "registry/semantic.rs"]
mod semantic;

#[derive(Debug)]
pub(super) enum SessionError {
    NotFound(String),
    Conflict(String),
    InvalidState(String),
    ResourceLimit(String),
    InvalidInput(String),
    Timeout(String),
    Lease(LeaseError),
    Internal(Error),
}

impl std::fmt::Display for SessionError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotFound(message)
            | Self::Conflict(message)
            | Self::InvalidState(message)
            | Self::ResourceLimit(message)
            | Self::InvalidInput(message)
            | Self::Timeout(message) => formatter.write_str(message),
            Self::Lease(error) => write!(formatter, "{error}"),
            Self::Internal(error) => write!(formatter, "{error:#}"),
        }
    }
}

impl From<Error> for SessionError {
    fn from(error: Error) -> Self {
        Self::Internal(error)
    }
}

impl From<LeaseError> for SessionError {
    fn from(error: LeaseError) -> Self {
        Self::Lease(error)
    }
}

#[derive(Default)]
pub(super) struct RegistryState {
    pub(super) sessions: HashMap<String, ManagedSession>,
    tombstones: VecDeque<String>,
}

pub(super) struct SessionRegistry {
    pub(super) state: Mutex<RegistryState>,
    shutting_down: AtomicBool,
    pub(super) events: std::sync::Arc<EventHub>,
    operations: Mutex<OperationStore>,
    pub(super) leases: LeaseManager,
    pub(super) drivers: DriverRegistry,
    session_gates: Mutex<HashMap<String, Arc<Mutex<()>>>>,
}

impl Default for SessionRegistry {
    fn default() -> Self {
        Self {
            state: Mutex::new(RegistryState::default()),
            shutting_down: AtomicBool::new(false),
            events: std::sync::Arc::new(EventHub::default()),
            operations: Mutex::new(OperationStore::default()),
            leases: LeaseManager::default(),
            drivers: DriverRegistry::with_builtins(),
            session_gates: Mutex::new(HashMap::new()),
        }
    }
}

impl SessionRegistry {
    pub(super) fn is_shutting_down(&self) -> bool {
        self.shutting_down.load(Ordering::Acquire)
    }

    fn gate_for(&self, session_id: &str) -> Arc<Mutex<()>> {
        let mut gates = self
            .session_gates
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        gates
            .entry(session_id.to_owned())
            .or_insert_with(|| Arc::new(Mutex::new(())))
            .clone()
    }

    fn remove_gate(&self, session_id: &str) {
        let mut gates = self
            .session_gates
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        gates.remove(session_id);
    }

    fn with_session_gate<F, T>(&self, session_id: String, f: F) -> Result<T, SessionError>
    where
        F: FnOnce(String) -> Result<T, SessionError>,
    {
        let gate = self.gate_for(&session_id);
        let _guard = gate.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        f(session_id)
    }

    pub(super) fn list(&self) -> std::result::Result<Vec<SessionInfo>, SessionError> {
        let mut state = self.state.lock().expect("session registry poisoned");
        refresh_all(&mut state, &self.events, &self.leases)?;
        let mut result = state
            .sessions
            .values()
            .map(|session| session.info.clone())
            .collect::<Vec<_>>();
        result.sort_by(|left, right| left.session_id.cmp(&right.session_id));
        Ok(result)
    }

    pub(super) fn start(
        &self,
        request: StartSession,
    ) -> std::result::Result<SessionInfo, SessionError> {
        validate_start(&request).map_err(SessionError::InvalidInput)?;
        let mut state = self.state.lock().expect("session registry poisoned");
        if self.is_shutting_down() {
            return Err(SessionError::InvalidState("daemon is shutting down".into()));
        }
        refresh_all(&mut state, &self.events, &self.leases)?;
        if state.sessions.contains_key(&request.session_id) {
            return Err(SessionError::Conflict(format!(
                "session already exists: {}",
                request.session_id
            )));
        }
        if state.sessions.len() >= MAX_SESSIONS {
            let evicted = evict_oldest_tombstone(&mut state).ok_or_else(|| {
                SessionError::ResourceLimit(format!(
                    "maximum session count reached: {MAX_SESSIONS}"
                ))
            })?;
            self.remove_gate(&evicted);
            self.events.remove_history(&evicted);
        }

        let session = ManagedSession::spawn(request)?;
        let info = session.info.clone();
        state.sessions.insert(info.session_id.clone(), session);
        self.events.publish(
            info.session_id.clone(),
            SessionEventKind::Lifecycle {
                state: info.state.clone(),
                exit_code: info.exit_code,
            },
        );
        let session = state.sessions.get_mut(&info.session_id).ok_or_else(|| {
            SessionError::Internal(anyhow::anyhow!("inserted session disappeared"))
        })?;
        session.start_output_drain(std::sync::Arc::clone(&self.events))?;
        Ok(info)
    }

    pub(super) fn inspect(
        &self,
        selector: SessionSelector,
    ) -> std::result::Result<SessionInfo, SessionError> {
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
            publish_lifecycle(&self.events, &info);
            record_tombstone(&mut state, selector.session_id.clone());
            self.leases.release_session(&selector.session_id);
        }
        Ok(info)
    }

    pub(super) fn stop(
        &self,
        selector: SessionSelector,
    ) -> std::result::Result<SessionInfo, SessionError> {
        let session_id = selector.session_id.clone();
        self.with_session_gate(session_id, |session_id| {
            let mut state = self.state.lock().expect("session registry poisoned");
            let (info, transitioned) = {
                let session = state.sessions.get_mut(&session_id).ok_or_else(|| {
                    SessionError::NotFound(format!("unknown session: {}", session_id))
                })?;
                let transitioned = session.stop()?;
                (session.info.clone(), transitioned)
            };
            if transitioned {
                publish_lifecycle(&self.events, &info);
                record_tombstone(&mut state, session_id.clone());
            }
            self.leases.release_session(&session_id);
            self.remove_gate(&session_id);
            Ok(info)
        })
    }

    pub(super) fn write(&self, request: TerminalWrite) -> std::result::Result<usize, SessionError> {
        self.write_bytes(TerminalWriteBytes {
            session_id: request.session_id,
            data: request.data.into_bytes(),
        })
    }

    pub(super) fn write_bytes(
        &self,
        request: TerminalWriteBytes,
    ) -> std::result::Result<usize, SessionError> {
        let session_id = request.session_id.clone();
        self.with_session_gate(session_id, |_session_id| {
            let mut state = self.state.lock().expect("session registry poisoned");
            let session = state.sessions.get_mut(&request.session_id).ok_or_else(|| {
                SessionError::NotFound(format!("unknown session: {}", request.session_id))
            })?;
            let transitioned = session.refresh()?;
            let written = session.write_running(&request.data);
            if transitioned {
                let info = session.info.clone();
                publish_lifecycle(&self.events, &info);
                record_tombstone(&mut state, info.session_id.clone());
                self.leases.release_session(&info.session_id);
            }
            written
        })
    }

    pub(super) fn key(&self, request: TerminalKey) -> std::result::Result<usize, SessionError> {
        let bytes =
            encode_key(&request.key, &request.modifiers).map_err(SessionError::InvalidInput)?;
        self.write_bytes(TerminalWriteBytes {
            session_id: request.session_id,
            data: bytes,
        })
    }

    pub(super) fn resize(
        &self,
        request: TerminalResize,
    ) -> std::result::Result<TerminalSnapshot, SessionError> {
        validate_dimensions(request.columns, request.rows).map_err(SessionError::InvalidInput)?;
        let session_id = request.session_id.clone();
        self.with_session_gate(session_id, |session_id| {
            let mut state = self.state.lock().expect("session registry poisoned");
            let session = state.sessions.get_mut(&session_id).ok_or_else(|| {
                SessionError::NotFound(format!("unknown session: {}", session_id))
            })?;
            let transitioned = session.refresh()?;
            let result = session.resize(request);
            if transitioned {
                let info = session.info.clone();
                publish_lifecycle(&self.events, &info);
                record_tombstone(&mut state, session_id);
                self.leases.release_session(&info.session_id);
            }
            result
        })
    }

    pub(super) fn shutdown(&self) {
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

fn refresh_all(
    state: &mut RegistryState,
    events: &EventHub,
    leases: &LeaseManager,
) -> std::result::Result<(), SessionError> {
    let ids = state.sessions.keys().cloned().collect::<Vec<_>>();
    for session_id in ids {
        let Some(session) = state.sessions.get_mut(&session_id) else {
            continue;
        };
        let transitioned = session.refresh()?;
        if transitioned {
            publish_lifecycle(events, &session.info);
            record_tombstone(state, session_id.clone());
            leases.release_session(&session_id);
        }
    }
    trim_tombstones(state);
    Ok(())
}

fn publish_lifecycle(events: &EventHub, info: &SessionInfo) {
    events.publish(
        info.session_id.clone(),
        SessionEventKind::Lifecycle {
            state: info.state.clone(),
            exit_code: info.exit_code,
        },
    );
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

fn evict_oldest_tombstone(state: &mut RegistryState) -> Option<String> {
    while let Some(session_id) = state.tombstones.pop_front() {
        if state.sessions.remove(&session_id).is_some() {
            return Some(session_id);
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
    validate_dimensions(request.columns, request.rows)
}

fn validate_dimensions(columns: u16, rows: u16) -> std::result::Result<(), String> {
    if columns == 0 || rows == 0 || columns > MAX_COLUMNS || rows > MAX_ROWS {
        return Err(format!(
            "terminal dimensions must be within 1..={MAX_COLUMNS} columns and 1..={MAX_ROWS} rows"
        ));
    }
    Ok(())
}

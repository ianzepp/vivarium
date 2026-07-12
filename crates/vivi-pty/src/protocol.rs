use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::io::{self, Read, Write};

pub const PROTOCOL_VERSION: u32 = 1;
pub const MAX_FRAME_BYTES: usize = 8 * 1024 * 1024;

pub mod error_codes {
    pub const INVALID_REQUEST: i32 = -32600;
    pub const METHOD_NOT_FOUND: i32 = -32601;
    pub const INVALID_PARAMS: i32 = -32602;
    pub const INTERNAL: i32 = -32603;
    pub const SESSION_NOT_FOUND: i32 = -32004;
    pub const SESSION_CONFLICT: i32 = -32009;
    pub const INVALID_STATE: i32 = -32010;
    pub const RESOURCE_LIMIT: i32 = -32011;
    pub const TIMEOUT: i32 = -32012;
    pub const EVENT_LAGGED: i32 = -32013;
    pub const OPERATION_CONFLICT: i32 = -32014;
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Request {
    pub jsonrpc: String,
    pub id: Value,
    pub method: String,
    #[serde(default)]
    pub params: Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub operation_id: Option<String>,
}

impl Request {
    pub fn new(id: u64, method: impl Into<String>, params: Value) -> Self {
        Self {
            jsonrpc: "2.0".into(),
            id: id.into(),
            method: method.into(),
            params,
            operation_id: None,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Response {
    pub jsonrpc: String,
    pub id: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<RpcError>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub operation_id: Option<String>,
}

impl Response {
    pub fn success(id: Value, result: impl Serialize) -> Self {
        Self {
            jsonrpc: "2.0".into(),
            id,
            result: Some(serde_json::to_value(result).expect("serializable RPC result")),
            error: None,
            operation_id: None,
        }
    }

    pub fn error(id: Value, code: i32, message: impl Into<String>) -> Self {
        Self {
            jsonrpc: "2.0".into(),
            id,
            result: None,
            error: Some(RpcError {
                code,
                message: message.into(),
                data: None,
            }),
            operation_id: None,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ServerNotification {
    pub jsonrpc: String,
    pub method: String,
    pub params: Value,
}

impl ServerNotification {
    pub fn event(batch: EventBatch) -> serde_json::Result<Self> {
        Ok(Self {
            jsonrpc: "2.0".into(),
            method: "session.event".into(),
            params: serde_json::to_value(batch)?,
        })
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct RpcError {
    pub code: i32,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct DaemonInfo {
    pub name: String,
    pub version: String,
    pub protocol_version: u32,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct StartSession {
    pub session_id: String,
    pub driver: String,
    pub command: Vec<String>,
    pub cwd: String,
    #[serde(default = "default_columns")]
    pub columns: u16,
    #[serde(default = "default_rows")]
    pub rows: u16,
}

fn default_columns() -> u16 {
    120
}

fn default_rows() -> u16 {
    40
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct SessionSelector {
    pub session_id: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct TerminalWrite {
    pub session_id: String,
    pub data: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct TerminalWriteBytes {
    pub session_id: String,
    pub data: Vec<u8>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum KeyModifier {
    Control,
    Alt,
    Shift,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct TerminalKey {
    pub session_id: String,
    pub key: String,
    #[serde(default)]
    pub modifiers: Vec<KeyModifier>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct TerminalResize {
    pub session_id: String,
    pub columns: u16,
    pub rows: u16,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct TerminalModes {
    pub alternate_screen: bool,
    pub application_keypad: bool,
    pub application_cursor: bool,
    pub cursor_hidden: bool,
    pub bracketed_paste: bool,
    pub mouse_protocol: String,
    pub mouse_encoding: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct TerminalSnapshot {
    pub session_id: String,
    pub columns: u16,
    pub rows: u16,
    pub cursor_column: u16,
    pub cursor_row: u16,
    pub contents: String,
    pub formatted_contents: Vec<u8>,
    pub scrollback: usize,
    pub scrollback_limit: usize,
    pub modes: TerminalModes,
    pub screen_revision: u64,
    pub output_sequence: u64,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct DiagnosticSnapshot {
    pub protocol: DaemonInfo,
    pub session: SessionInfo,
    pub terminal: TerminalSnapshot,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionState {
    Running,
    Exited,
    Stopped,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct SessionInfo {
    pub session_id: String,
    pub driver: String,
    pub command: Vec<String>,
    pub cwd: String,
    pub pid: Option<u32>,
    pub state: SessionState,
    pub exit_code: Option<u32>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionEventKind {
    Lifecycle {
        state: SessionState,
        exit_code: Option<u32>,
    },
    Screen {
        screen_revision: u64,
        output_sequence: u64,
    },
    Operation {
        operation_id: String,
        method: String,
        success: bool,
        error_code: Option<i32>,
    },
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct SessionEvent {
    pub session_id: String,
    pub sequence: u64,
    pub kind: SessionEventKind,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct EventBatch {
    pub session_id: String,
    pub events: Vec<SessionEvent>,
    pub latest_sequence: u64,
    pub lagged: bool,
    pub snapshot: Option<DiagnosticSnapshot>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct SessionSubscribe {
    pub session_id: String,
    #[serde(default)]
    pub after_sequence: u64,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct SubscriptionAck {
    pub session_id: String,
    pub next_sequence: u64,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct SessionWait {
    pub session_id: String,
    pub state: Option<SessionState>,
    pub screen_revision: Option<u64>,
    pub event_sequence: Option<u64>,
    #[serde(default = "default_wait_timeout_ms")]
    pub timeout_ms: u64,
}

fn default_wait_timeout_ms() -> u64 {
    30_000
}

pub use crate::keys::encode_key;

pub fn write_frame<T: Serialize>(writer: &mut impl Write, message: &T) -> io::Result<()> {
    let payload = serde_json::to_vec(message).map_err(io::Error::other)?;
    if payload.len() > MAX_FRAME_BYTES {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "frame too large",
        ));
    }
    writer.write_all(&(payload.len() as u32).to_be_bytes())?;
    writer.write_all(&payload)?;
    writer.flush()
}

pub fn read_frame<T: for<'de> Deserialize<'de>>(reader: &mut impl Read) -> io::Result<T> {
    let mut header = [0_u8; 4];
    reader.read_exact(&mut header)?;
    let length = u32::from_be_bytes(header) as usize;
    if length > MAX_FRAME_BYTES {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "frame too large",
        ));
    }
    let mut payload = vec![0; length];
    reader.read_exact(&mut payload)?;
    serde_json::from_slice(&payload).map_err(io::Error::other)
}

#[cfg(test)]
#[path = "protocol_test.rs"]
mod tests;

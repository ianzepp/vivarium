use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeKind {
    Tmux,
    ViviPty,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct RuntimeBinding {
    pub role: String,
    pub mail_identity: String,
    pub runtime: RuntimeKind,
    pub session_id: String,
    pub socket: Option<PathBuf>,
    pub driver: Option<String>,
    pub cwd: PathBuf,
    pub command: Vec<String>,
    pub tmux_target: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum BindingError {
    MissingRole(String),
    Invalid(String),
}

impl std::fmt::Display for BindingError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MissingRole(role) => write!(formatter, "Fleet role is not configured: {role}"),
            Self::Invalid(message) => formatter.write_str(message),
        }
    }
}

impl std::error::Error for BindingError {}

pub fn resolve_role(
    config: &Value,
    role: &str,
    project: impl AsRef<Path>,
) -> Result<RuntimeBinding, BindingError> {
    validate_identity(role, "role")?;
    let project = project.as_ref();
    let slot = config
        .get("hands")
        .and_then(|hands| hands.get(role))
        .or_else(|| config.get(role))
        .ok_or_else(|| BindingError::MissingRole(role.into()))?;
    let mail_identity = string_field(slot, "mail_identity")?.unwrap_or_else(|| role.into());
    validate_identity(&mail_identity, "mail_identity")?;
    let cwd = string_field(slot, "cwd")?
        .map(PathBuf::from)
        .unwrap_or_else(|| project.to_path_buf());
    let runtime = runtime_object(slot)?;
    let session_id = runtime
        .and_then(|runtime| runtime.get("session_id"))
        .and_then(Value::as_str)
        .unwrap_or(&mail_identity)
        .to_owned();
    validate_identity(&session_id, "session_id")?;
    match runtime_kind_name(runtime)? {
        RuntimeKind::Tmux => resolve_tmux(role, mail_identity, session_id, cwd, slot),
        RuntimeKind::ViviPty => {
            resolve_vivi_pty(role, mail_identity, session_id, cwd, project, slot, runtime)
        }
    }
}

fn resolve_tmux(
    role: &str,
    mail_identity: String,
    session_id: String,
    cwd: PathBuf,
    slot: &Value,
) -> Result<RuntimeBinding, BindingError> {
    let target = string_field(slot, "tmux_target")?
        .ok_or_else(|| BindingError::Invalid(format!("Fleet role {role} has no tmux_target")))?;
    if target.trim().is_empty() {
        return Err(BindingError::Invalid(format!(
            "Fleet role {role} has an empty tmux_target"
        )));
    }
    Ok(RuntimeBinding {
        role: role.into(),
        mail_identity,
        runtime: RuntimeKind::Tmux,
        session_id,
        socket: None,
        driver: None,
        cwd,
        command: Vec::new(),
        tmux_target: Some(target),
    })
}

fn resolve_vivi_pty(
    role: &str,
    mail_identity: String,
    session_id: String,
    cwd: PathBuf,
    project: &Path,
    slot: &Value,
    runtime: Option<&Value>,
) -> Result<RuntimeBinding, BindingError> {
    for field in ["tmux_target", "tmux_session", "tmux_window"] {
        if string_field(slot, field)?.is_some() || slot.get(field).is_some() {
            return Err(BindingError::Invalid(format!(
                "Vivi PTY role {role} cannot also define {field}"
            )));
        }
    }
    let socket = runtime
        .and_then(|runtime| runtime.get("socket"))
        .and_then(Value::as_str)
        .map(PathBuf::from)
        .or_else(|| Some(project.join(".vivi/vivi-pty.sock")));
    let driver = runtime
        .and_then(|runtime| runtime.get("driver"))
        .and_then(Value::as_str)
        .or_else(|| slot.get("agent").and_then(Value::as_str))
        .map(str::to_owned);
    let command = runtime
        .and_then(|runtime| runtime.get("command"))
        .and_then(Value::as_array)
        .ok_or_else(|| {
            BindingError::Invalid(format!("Vivi PTY role {role} needs runtime.command"))
        })?
        .iter()
        .map(|value| {
            value.as_str().map(str::to_owned).ok_or_else(|| {
                BindingError::Invalid(format!("Vivi PTY role {role} command must be strings"))
            })
        })
        .collect::<Result<Vec<_>, _>>()?;
    if command.is_empty() || command[0].trim().is_empty() {
        return Err(BindingError::Invalid(format!(
            "Vivi PTY role {role} command cannot be empty"
        )));
    }
    Ok(RuntimeBinding {
        role: role.into(),
        mail_identity,
        runtime: RuntimeKind::ViviPty,
        session_id,
        socket,
        driver,
        cwd,
        command,
        tmux_target: None,
    })
}

fn runtime_object(slot: &Value) -> Result<Option<&Value>, BindingError> {
    match slot.get("runtime") {
        None => Ok(None),
        Some(Value::Object(_)) => Ok(slot.get("runtime")),
        Some(_) => Err(BindingError::Invalid(
            "runtime must be an object with a kind field".into(),
        )),
    }
}

fn runtime_kind_name(runtime: Option<&Value>) -> Result<RuntimeKind, BindingError> {
    let Some(runtime) = runtime else {
        return Ok(RuntimeKind::Tmux);
    };
    let kind = runtime
        .get("kind")
        .and_then(Value::as_str)
        .ok_or_else(|| BindingError::Invalid("runtime.kind is required".into()))?;
    match kind {
        "tmux" => Ok(RuntimeKind::Tmux),
        "vivi_pty" => Ok(RuntimeKind::ViviPty),
        other => Err(BindingError::Invalid(format!(
            "unsupported runtime kind: {other}"
        ))),
    }
}

fn string_field(value: &Value, field: &str) -> Result<Option<String>, BindingError> {
    match value.get(field) {
        None => Ok(None),
        Some(Value::String(value)) => Ok(Some(value.clone())),
        Some(_) => Err(BindingError::Invalid(format!("{field} must be a string"))),
    }
}

fn validate_identity(value: &str, field: &str) -> Result<(), BindingError> {
    if value.is_empty()
        || value.len() > 128
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
    {
        return Err(BindingError::Invalid(format!(
            "{field} must be a bounded alphanumeric identity"
        )));
    }
    Ok(())
}

#[cfg(test)]
#[path = "binding_test.rs"]
mod tests;

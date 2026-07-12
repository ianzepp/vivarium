use crate::operation::validate_operation_id;
use crate::protocol::{KeyModifier, TerminalSnapshot};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum HarnessState {
    Starting,
    WaitingForInput,
    Submitting,
    Running,
    ApprovalRequired,
    Completed,
    Failed,
    Stopped,
    Unknown,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Confidence {
    Low,
    Medium,
    High,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Evidence {
    pub source: String,
    pub detail: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Classification {
    pub state: HarnessState,
    pub confidence: Confidence,
    pub evidence: Vec<Evidence>,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct Capabilities {
    pub submit: bool,
    pub interrupt: bool,
    pub approve: bool,
    pub reject: bool,
    pub restart: bool,
    pub raw_input: bool,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum TerminalAction {
    WriteText(String),
    WriteBytes(Vec<u8>),
    Key {
        key: String,
        modifiers: Vec<KeyModifier>,
    },
    WaitForState(HarnessState),
    WaitForScreenSettle,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum SemanticAction {
    Submit { message: String },
    Interrupt,
    Approve,
    Reject,
    Restart,
    Raw(TerminalAction),
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ActionRequest {
    pub operation_id: String,
    pub action: SemanticAction,
    pub expected_state: Option<HarnessState>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct PlannedAction {
    pub operation_id: String,
    pub actions: Vec<TerminalAction>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ActionOutcome {
    pub operation_id: String,
    pub state: HarnessState,
    pub evidence: Vec<Evidence>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DriverError {
    UnknownDriver(String),
    Unsupported {
        driver: String,
        action: String,
    },
    StateMismatch {
        expected: HarnessState,
        actual: HarnessState,
    },
    Busy(String),
    UnknownOperation(String),
    InvalidOperationId(String),
}

impl std::fmt::Display for DriverError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnknownDriver(name) => write!(formatter, "unknown driver: {name}"),
            Self::Unsupported { driver, action } => {
                write!(formatter, "driver {driver} does not support {action}")
            }
            Self::StateMismatch { expected, actual } => {
                write!(formatter, "expected {expected:?}, observed {actual:?}")
            }
            Self::Busy(operation_id) => write!(formatter, "session is busy with {operation_id}"),
            Self::UnknownOperation(operation_id) => {
                write!(formatter, "unknown operation: {operation_id}")
            }
            Self::InvalidOperationId(message) => formatter.write_str(message),
        }
    }
}

pub trait HarnessDriver: Send + Sync {
    fn name(&self) -> &'static str;
    fn capabilities(&self) -> Capabilities;
    fn classify(&self, terminal: &TerminalSnapshot) -> Classification;
    fn plan(&self, action: &SemanticAction) -> Result<Vec<TerminalAction>, DriverError>;
}

#[derive(Clone, Debug, Default)]
pub struct GenericDriver;

impl HarnessDriver for GenericDriver {
    fn name(&self) -> &'static str {
        "generic"
    }

    fn capabilities(&self) -> Capabilities {
        Capabilities {
            submit: true,
            interrupt: true,
            raw_input: true,
            ..Capabilities::default()
        }
    }

    fn classify(&self, terminal: &TerminalSnapshot) -> Classification {
        let visible = terminal.contents.trim();
        if visible.is_empty() {
            return classification(HarnessState::Unknown, Confidence::Low, "screen is empty");
        }
        let last_line = terminal
            .contents
            .lines()
            .rev()
            .find(|line| !line.trim().is_empty());
        if last_line.is_some_and(|line| line.ends_with("$ ") || line.ends_with("# ")) {
            return classification(
                HarnessState::WaitingForInput,
                Confidence::High,
                "visible shell prompt",
            );
        }
        classification(
            HarnessState::Running,
            Confidence::Medium,
            "visible output without a recognized prompt",
        )
    }

    fn plan(&self, action: &SemanticAction) -> Result<Vec<TerminalAction>, DriverError> {
        match action {
            SemanticAction::Submit { message } => Ok(vec![
                TerminalAction::WriteText(message.clone()),
                TerminalAction::Key {
                    key: "Enter".into(),
                    modifiers: Vec::new(),
                },
                TerminalAction::WaitForState(HarnessState::Running),
            ]),
            SemanticAction::Interrupt => Ok(vec![TerminalAction::Key {
                key: "c".into(),
                modifiers: vec![KeyModifier::Control],
            }]),
            SemanticAction::Raw(action) => Ok(vec![action.clone()]),
            SemanticAction::Approve => Err(DriverError::Unsupported {
                driver: self.name().into(),
                action: "approve".into(),
            }),
            SemanticAction::Reject => Err(DriverError::Unsupported {
                driver: self.name().into(),
                action: "reject".into(),
            }),
            SemanticAction::Restart => Err(DriverError::Unsupported {
                driver: self.name().into(),
                action: "restart".into(),
            }),
        }
    }
}

fn classification(state: HarnessState, confidence: Confidence, detail: &str) -> Classification {
    Classification {
        state,
        confidence,
        evidence: vec![Evidence {
            source: "terminal".into(),
            detail: detail.into(),
        }],
    }
}

#[derive(Default)]
pub struct DriverRegistry {
    drivers: HashMap<String, Arc<dyn HarnessDriver>>,
}

impl DriverRegistry {
    pub fn with_generic() -> Self {
        let mut registry = Self::default();
        registry.register(Arc::new(GenericDriver));
        registry
    }

    pub fn with_builtins() -> Self {
        let mut registry = Self::with_generic();
        registry.register(Arc::new(crate::codex::CodexDriver));
        registry.register(Arc::new(crate::grok::GrokDriver));
        registry.register(Arc::new(crate::opencode::OpenCodeDriver));
        registry.register(Arc::new(crate::pi::PiDriver));
        registry
    }

    pub fn register(&mut self, driver: Arc<dyn HarnessDriver>) {
        self.drivers.insert(driver.name().into(), driver);
    }

    pub fn get(&self, name: &str) -> Result<Arc<dyn HarnessDriver>, DriverError> {
        self.drivers
            .get(name)
            .cloned()
            .ok_or_else(|| DriverError::UnknownDriver(name.into()))
    }
}

#[derive(Default)]
pub struct ActionQueue {
    active_operation: Mutex<Option<String>>,
}

impl ActionQueue {
    pub fn start(
        &self,
        request: ActionRequest,
        classification: &Classification,
        driver: &dyn HarnessDriver,
    ) -> Result<PlannedAction, DriverError> {
        validate_operation_id(&request.operation_id).map_err(DriverError::InvalidOperationId)?;
        if let Some(expected) = request.expected_state.clone()
            && expected != classification.state
        {
            return Err(DriverError::StateMismatch {
                expected,
                actual: classification.state.clone(),
            });
        }
        let mut active = self
            .active_operation
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if let Some(operation_id) = active.as_ref() {
            return Err(DriverError::Busy(operation_id.clone()));
        }
        if !capability_allows(&request.action, driver.capabilities()) {
            return Err(DriverError::Unsupported {
                driver: driver.name().into(),
                action: action_name(&request.action).into(),
            });
        }
        let actions = driver.plan(&request.action)?;
        *active = Some(request.operation_id.clone());
        Ok(PlannedAction {
            operation_id: request.operation_id,
            actions,
        })
    }

    pub fn complete(
        &self,
        operation_id: &str,
        classification: &Classification,
    ) -> Result<ActionOutcome, DriverError> {
        self.clear_active(operation_id)?;
        Ok(ActionOutcome {
            operation_id: operation_id.into(),
            state: classification.state.clone(),
            evidence: classification.evidence.clone(),
        })
    }

    /// Acquire the per-session semantic lock without a driver plan.
    /// Used for daemon-owned operations such as process-group restart.
    pub fn begin_exclusive(&self, operation_id: &str) -> Result<(), DriverError> {
        validate_operation_id(operation_id).map_err(DriverError::InvalidOperationId)?;
        let mut active = self
            .active_operation
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if let Some(existing) = active.as_ref() {
            return Err(DriverError::Busy(existing.clone()));
        }
        *active = Some(operation_id.into());
        Ok(())
    }

    pub fn clear_active(&self, operation_id: &str) -> Result<(), DriverError> {
        let mut active = self
            .active_operation
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if active.as_deref() != Some(operation_id) {
            return Err(DriverError::UnknownOperation(operation_id.into()));
        }
        *active = None;
        Ok(())
    }
}

fn capability_allows(action: &SemanticAction, capabilities: Capabilities) -> bool {
    match action {
        SemanticAction::Submit { .. } => capabilities.submit,
        SemanticAction::Interrupt => capabilities.interrupt,
        SemanticAction::Approve => capabilities.approve,
        SemanticAction::Reject => capabilities.reject,
        SemanticAction::Restart => capabilities.restart,
        SemanticAction::Raw(_) => capabilities.raw_input,
    }
}

fn action_name(action: &SemanticAction) -> &'static str {
    match action {
        SemanticAction::Submit { .. } => "submit",
        SemanticAction::Interrupt => "interrupt",
        SemanticAction::Approve => "approve",
        SemanticAction::Reject => "reject",
        SemanticAction::Restart => "restart",
        SemanticAction::Raw(_) => "raw",
    }
}

#[cfg(test)]
#[path = "driver_test.rs"]
mod tests;

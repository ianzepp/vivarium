use crate::driver::{
    Capabilities, Classification, Confidence, DriverError, HarnessDriver, HarnessState,
    SemanticAction, TerminalAction,
};
use crate::protocol::{KeyModifier, TerminalSnapshot};

#[derive(Clone, Debug, Default)]
pub struct OpenCodeDriver;

impl HarnessDriver for OpenCodeDriver {
    fn name(&self) -> &'static str {
        "opencode"
    }

    fn capabilities(&self) -> Capabilities {
        Capabilities {
            submit: true,
            interrupt: true,
            approve: true,
            reject: true,
            raw_input: true,
            ..Capabilities::default()
        }
    }

    fn classify(&self, terminal: &TerminalSnapshot) -> Classification {
        let visible = terminal.contents.trim();
        if visible.is_empty() {
            return evidence(HarnessState::Starting, Confidence::Low, "screen is empty");
        }
        if contains_any(
            visible,
            &[
                "Do you trust",
                "Always allow",
                "Allow once",
                "until OpenCode is restarted",
            ],
        ) {
            return evidence(
                HarnessState::ApprovalRequired,
                Confidence::High,
                "visible OpenCode trust prompt",
            );
        }
        if contains_any(
            visible,
            &[
                "connection failed",
                "request timed out",
                "stream timed out",
                "ECONNRESET",
            ],
        ) {
            return evidence(
                HarnessState::Failed,
                Confidence::High,
                "visible OpenCode connection error",
            );
        }
        if contains_any(
            visible,
            &["Turn completed", "Idle until", "Board empty", "bag empty"],
        ) {
            return evidence(
                HarnessState::Completed,
                Confidence::High,
                "visible OpenCode completion marker",
            );
        }
        if last_line_is_prompt(visible) {
            return evidence(
                HarnessState::WaitingForInput,
                Confidence::High,
                "visible OpenCode composer prompt",
            );
        }
        if contains_any(visible, &["⬝", "esc interrupt", "Build ·", "Responding"]) {
            return evidence(
                HarnessState::Running,
                Confidence::Medium,
                "visible OpenCode activity marker",
            );
        }
        evidence(
            HarnessState::Unknown,
            Confidence::Low,
            "screen lacks a stable OpenCode marker",
        )
    }

    fn plan(&self, action: &SemanticAction) -> Result<Vec<TerminalAction>, DriverError> {
        match action {
            SemanticAction::Submit { message } => Ok(submit_actions(message)),
            SemanticAction::Interrupt => Ok(ctrl_c()),
            SemanticAction::Approve => Ok(key("Enter")),
            SemanticAction::Reject => Ok(key("Escape")),
            SemanticAction::Raw(action) => Ok(vec![action.clone()]),
            SemanticAction::Restart => Err(DriverError::Unsupported {
                driver: self.name().into(),
                action: "restart".into(),
            }),
        }
    }
}

fn evidence(state: HarnessState, confidence: Confidence, detail: &str) -> Classification {
    Classification {
        state,
        confidence,
        evidence: vec![crate::driver::Evidence {
            source: "terminal".into(),
            detail: detail.into(),
        }],
    }
}

fn contains_any(value: &str, markers: &[&str]) -> bool {
    let lower = value.to_ascii_lowercase();
    markers
        .iter()
        .any(|marker| lower.contains(&marker.to_ascii_lowercase()))
}

fn last_line_is_prompt(value: &str) -> bool {
    value
        .lines()
        .rev()
        .find(|line| !line.trim().is_empty())
        .is_some_and(|line| line.contains("Ask anything..."))
}

fn submit_actions(message: &str) -> Vec<TerminalAction> {
    vec![
        TerminalAction::WriteText(message.into()),
        TerminalAction::WaitForScreenSettle,
        TerminalAction::Key {
            key: "Enter".into(),
            modifiers: Vec::new(),
        },
        TerminalAction::WaitForState(HarnessState::Running),
    ]
}

fn ctrl_c() -> Vec<TerminalAction> {
    vec![TerminalAction::Key {
        key: "c".into(),
        modifiers: vec![KeyModifier::Control],
    }]
}

fn key(name: &str) -> Vec<TerminalAction> {
    vec![TerminalAction::Key {
        key: name.into(),
        modifiers: Vec::new(),
    }]
}

#[cfg(test)]
#[path = "opencode_test.rs"]
mod tests;

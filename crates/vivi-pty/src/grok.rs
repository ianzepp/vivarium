use crate::driver::{
    Capabilities, Classification, Confidence, DriverError, HarnessDriver, HarnessState,
    SemanticAction, TerminalAction,
};
use crate::protocol::TerminalSnapshot;

#[derive(Clone, Debug, Default)]
pub struct GrokDriver;

impl HarnessDriver for GrokDriver {
    fn name(&self) -> &'static str {
        "grok"
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
            return evidence(HarnessState::Starting, Confidence::Low, "screen is empty");
        }
        if contains_any(
            visible,
            &[
                "rate limit",
                "over capacity",
                "connection failed",
                "request timed out",
            ],
        ) {
            return evidence(
                HarnessState::Failed,
                Confidence::High,
                "visible Grok capacity or connection error",
            );
        }
        if visible.contains("Grok Build") || visible.contains("Grok") {
            if contains_any(visible, &["Working", "Responding", "Thinking"]) {
                return evidence(
                    HarnessState::Running,
                    Confidence::High,
                    "Grok is actively working",
                );
            }
            if grok_tui_waiting(visible) {
                return evidence(
                    HarnessState::WaitingForInput,
                    Confidence::High,
                    "Grok TUI prompt is visible",
                );
            }
            return evidence(
                HarnessState::Running,
                Confidence::Medium,
                "Grok TUI is visible without a clear prompt",
            );
        }
        if last_line_is_prompt(visible) {
            return evidence(
                HarnessState::WaitingForInput,
                Confidence::High,
                "visible prompt",
            );
        }
        evidence(
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
                modifiers: vec![crate::protocol::KeyModifier::Control],
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

fn contains_any(haystack: &str, needles: &[&str]) -> bool {
    needles.iter().any(|needle| haystack.contains(needle))
}

fn grok_tui_waiting(visible: &str) -> bool {
    visible
        .lines()
        .rev()
        .find(|line| !line.trim().is_empty())
        .is_some_and(|line| line.contains('❯'))
}

fn last_line_is_prompt(visible: &str) -> bool {
    visible
        .lines()
        .rev()
        .find(|line| !line.trim().is_empty())
        .is_some_and(|line| {
            let trimmed = line.trim_end();
            trimmed.ends_with("$ ")
                || trimmed.ends_with("# ")
                || trimmed.ends_with("> ")
                || trimmed.ends_with('$')
                || trimmed.ends_with('#')
                || trimmed.ends_with('>')
        })
}

#[cfg(test)]
#[path = "grok_test.rs"]
mod tests;

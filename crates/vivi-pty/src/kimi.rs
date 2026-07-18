use crate::driver::{
    Capabilities, Classification, Confidence, DriverError, HarnessDriver, HarnessState,
    SemanticAction, TerminalAction,
};
use crate::protocol::TerminalSnapshot;

/// Moon-phase spinner glyphs the Kimi Code TUI animates while a turn runs.
const SPINNER_GLYPHS: [char; 8] = ['🌑', '🌒', '🌓', '🌔', '🌕', '🌖', '🌗', '🌘'];

#[derive(Clone, Debug, Default)]
pub struct KimiDriver;

impl HarnessDriver for KimiDriver {
    fn name(&self) -> &'static str {
        "kimi"
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
                "Approve once",
                "Approve for this session",
                "Reject with feedback",
                "↑/↓ select",
            ],
        ) {
            return evidence(
                HarnessState::ApprovalRequired,
                Confidence::High,
                "visible Kimi Code approval panel",
            );
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
                "visible Kimi Code capacity or connection error",
            );
        }
        if visible.chars().any(|ch| SPINNER_GLYPHS.contains(&ch)) {
            return evidence(
                HarnessState::Running,
                Confidence::High,
                "Kimi Code turn spinner is visible",
            );
        }
        if kimi_composer_waiting(visible) {
            return evidence(
                HarnessState::WaitingForInput,
                Confidence::High,
                "Kimi Code composer prompt is visible",
            );
        }
        if visible.contains("Kimi Code") || visible.contains("context:") {
            return evidence(
                HarnessState::Running,
                Confidence::Medium,
                "Kimi Code TUI is visible without a clear prompt",
            );
        }
        if last_line_is_prompt(visible) {
            return evidence(
                HarnessState::WaitingForInput,
                Confidence::High,
                "visible shell prompt",
            );
        }
        evidence(
            HarnessState::Unknown,
            Confidence::Low,
            "screen lacks a stable Kimi Code marker",
        )
    }

    fn plan(&self, action: &SemanticAction) -> Result<Vec<TerminalAction>, DriverError> {
        match action {
            SemanticAction::Submit { message } => Ok(vec![
                TerminalAction::WriteText(message.clone()),
                TerminalAction::WaitForScreenSettle,
                TerminalAction::Key {
                    key: "Enter".into(),
                    modifiers: Vec::new(),
                },
                TerminalAction::WaitForState(HarnessState::Running),
            ]),
            SemanticAction::Interrupt => Ok(vec![TerminalAction::Key {
                key: "Escape".into(),
                modifiers: Vec::new(),
            }]),
            SemanticAction::Approve => Ok(vec![TerminalAction::Key {
                key: "Enter".into(),
                modifiers: Vec::new(),
            }]),
            SemanticAction::Reject => Ok(vec![TerminalAction::Key {
                key: "Escape".into(),
                modifiers: Vec::new(),
            }]),
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

fn contains_any(haystack: &str, needles: &[&str]) -> bool {
    needles.iter().any(|needle| haystack.contains(needle))
}

fn kimi_composer_waiting(visible: &str) -> bool {
    visible
        .lines()
        .rev()
        .filter(|line| !line.trim().is_empty())
        .take(8)
        .any(|line| {
            let trimmed = line.trim_start();
            trimmed.starts_with("│ >") || trimmed.starts_with("│ !")
        })
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
                || trimmed.ends_with('$')
                || trimmed.ends_with('#')
        })
}

#[cfg(test)]
#[path = "kimi_test.rs"]
mod tests;

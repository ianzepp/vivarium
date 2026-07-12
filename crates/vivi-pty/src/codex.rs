use crate::driver::{
    Capabilities, Classification, Confidence, DriverError, HarnessDriver, HarnessState,
    SemanticAction, TerminalAction,
};
use crate::operation::validate_operation_id;
use crate::protocol::TerminalSnapshot;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SubmissionPhase {
    AwaitingComposer,
    AwaitingOutcome,
    Running,
    Completed,
    Failed,
    Uncertain,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CodexSubmission {
    pub operation_id: String,
    pub message: String,
    pub baseline_screen_revision: u64,
    pub phase: SubmissionPhase,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SubmissionProgress {
    pub phase: SubmissionPhase,
    pub actions: Vec<TerminalAction>,
    pub classification: Classification,
}

#[derive(Clone, Debug, Default)]
pub struct CodexDriver;

impl CodexDriver {
    pub fn begin_submission(
        &self,
        operation_id: impl Into<String>,
        message: impl Into<String>,
        terminal: &TerminalSnapshot,
    ) -> Result<(CodexSubmission, Vec<TerminalAction>), DriverError> {
        let operation_id = operation_id.into();
        validate_operation_id(&operation_id).map_err(DriverError::InvalidOperationId)?;
        let classification = self.classify(terminal);
        if classification.state != HarnessState::WaitingForInput {
            return Err(DriverError::StateMismatch {
                expected: HarnessState::WaitingForInput,
                actual: classification.state,
            });
        }
        let submission = CodexSubmission {
            operation_id,
            message: message.into(),
            baseline_screen_revision: terminal.screen_revision,
            phase: SubmissionPhase::AwaitingComposer,
        };
        let actions = vec![
            TerminalAction::WriteText(submission.message.clone()),
            TerminalAction::WaitForScreenSettle,
        ];
        Ok((submission, actions))
    }

    pub fn advance_submission(
        &self,
        submission: &mut CodexSubmission,
        terminal: &TerminalSnapshot,
    ) -> SubmissionProgress {
        let classification = self.classify(terminal);
        let actions = match submission.phase {
            SubmissionPhase::AwaitingComposer => {
                if terminal.screen_revision <= submission.baseline_screen_revision {
                    Vec::new()
                } else if terminal.contents.contains(&submission.message) {
                    submission.phase = SubmissionPhase::AwaitingOutcome;
                    vec![
                        TerminalAction::Key {
                            key: "Enter".into(),
                            modifiers: Vec::new(),
                        },
                        TerminalAction::WaitForState(HarnessState::Running),
                    ]
                } else {
                    submission.phase = SubmissionPhase::Uncertain;
                    Vec::new()
                }
            }
            SubmissionPhase::AwaitingOutcome => match classification.state {
                HarnessState::Running => {
                    submission.phase = SubmissionPhase::Running;
                    Vec::new()
                }
                HarnessState::Completed => {
                    submission.phase = SubmissionPhase::Completed;
                    Vec::new()
                }
                HarnessState::Failed => {
                    submission.phase = SubmissionPhase::Failed;
                    Vec::new()
                }
                _ => {
                    submission.phase = SubmissionPhase::Uncertain;
                    Vec::new()
                }
            },
            _ => Vec::new(),
        };
        SubmissionProgress {
            phase: submission.phase.clone(),
            actions,
            classification,
        }
    }

    pub fn expire_submission(
        &self,
        submission: &mut CodexSubmission,
        terminal: &TerminalSnapshot,
    ) -> SubmissionProgress {
        submission.phase = SubmissionPhase::Uncertain;
        SubmissionProgress {
            phase: SubmissionPhase::Uncertain,
            actions: Vec::new(),
            classification: self.classify(terminal),
        }
    }
}

impl HarnessDriver for CodexDriver {
    fn name(&self) -> &'static str {
        "codex"
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
            return classification(HarnessState::Starting, Confidence::Low, "screen is empty");
        }
        let lower = visible.to_ascii_lowercase();
        if contains_any(
            &lower,
            &["allow this command", "press y to approve", "(y/n)"],
        ) {
            return classification(
                HarnessState::ApprovalRequired,
                Confidence::High,
                "visible Codex approval prompt",
            );
        }
        if contains_any(&lower, &["task completed", "successfully completed"]) {
            return classification(
                HarnessState::Completed,
                Confidence::High,
                "visible Codex completion marker",
            );
        }
        if contains_any(&lower, &["error:", "command failed", "task failed"]) {
            return classification(
                HarnessState::Failed,
                Confidence::High,
                "visible Codex failure marker",
            );
        }
        if last_line_is_composer(visible) {
            return classification(
                HarnessState::WaitingForInput,
                Confidence::High,
                "visible Codex composer prompt",
            );
        }
        if contains_any(&lower, &["thinking", "working", "running"]) {
            return classification(
                HarnessState::Running,
                Confidence::Medium,
                "visible Codex activity marker",
            );
        }
        classification(
            HarnessState::Unknown,
            Confidence::Low,
            "screen lacks a stable Codex state marker",
        )
    }

    fn plan(&self, action: &SemanticAction) -> Result<Vec<TerminalAction>, DriverError> {
        match action {
            SemanticAction::Submit { message } => Ok(vec![
                TerminalAction::WriteText(message.clone()),
                TerminalAction::WaitForScreenSettle,
            ]),
            SemanticAction::Interrupt => Ok(vec![TerminalAction::Key {
                key: "c".into(),
                modifiers: vec![crate::protocol::KeyModifier::Control],
            }]),
            SemanticAction::Approve => Ok(key_action("y")),
            SemanticAction::Reject => Ok(key_action("n")),
            SemanticAction::Raw(action) => Ok(vec![action.clone()]),
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
        evidence: vec![crate::driver::Evidence {
            source: "terminal".into(),
            detail: detail.into(),
        }],
    }
}

fn contains_any(value: &str, markers: &[&str]) -> bool {
    markers.iter().any(|marker| value.contains(marker))
}

fn last_line_is_composer(visible: &str) -> bool {
    visible
        .lines()
        .rev()
        .find(|line| !line.trim().is_empty())
        .is_some_and(|line| {
            let line = line.trim_start();
            line.starts_with("› ") || line == "›" || line.starts_with("> ") || line == ">"
        })
}

fn key_action(key: &str) -> Vec<TerminalAction> {
    vec![
        TerminalAction::Key {
            key: key.into(),
            modifiers: Vec::new(),
        },
        TerminalAction::Key {
            key: "Enter".into(),
            modifiers: Vec::new(),
        },
    ]
}

#[cfg(test)]
#[path = "codex_test.rs"]
mod tests;

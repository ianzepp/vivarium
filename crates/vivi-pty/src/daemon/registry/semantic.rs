use super::*;
use crate::codex::{CodexDriver, SubmissionPhase};
use crate::driver::{
    ActionRequest, Classification, DriverError, Evidence, HarnessState, SemanticAction,
    TerminalAction,
};
use crate::protocol::{
    SemanticEvidence, SemanticOutcome, SessionInterrupt, SessionRestart, SessionState,
    SessionSubmit,
};
use std::time::{Duration, Instant};

const MAX_SUBMIT_TIMEOUT_MS: u64 = 30_000;
const SEMANTIC_POLL: Duration = Duration::from_millis(20);

impl SessionRegistry {
    pub(in crate::daemon) fn submit(
        &self,
        request: SessionSubmit,
        operation_id: String,
    ) -> std::result::Result<SemanticOutcome, SessionError> {
        if request.message.is_empty() {
            return Err(SessionError::InvalidInput(
                "submit message cannot be empty".into(),
            ));
        }
        if request.timeout_ms == 0 || request.timeout_ms > MAX_SUBMIT_TIMEOUT_MS {
            return Err(SessionError::InvalidInput(format!(
                "submit timeout_ms must be 1..={MAX_SUBMIT_TIMEOUT_MS}"
            )));
        }
        validate_operation_id(&operation_id).map_err(SessionError::InvalidInput)?;

        let session_id = request.session_id.clone();
        self.with_session_gate(session_id, move |_session_id| {
            let mut state = self
                .state
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            let session = state.sessions.get_mut(&request.session_id).ok_or_else(|| {
                SessionError::NotFound(format!("unknown session: {}", request.session_id))
            })?;
            ensure_running(session)?;
            if session.info.driver != "codex" {
                return Err(SessionError::InvalidState(format!(
                    "daemon submission acknowledgement is only implemented for codex (session driver is {})",
                    session.info.driver
                )));
            }
            let driver = self
                .drivers
                .get(&session.info.driver)
                .map_err(map_driver_error)?;
            let snapshot = session.snapshot();
            let classification = driver.classify(&snapshot);
            session
                .actions
                .start(
                    ActionRequest {
                        operation_id: operation_id.clone(),
                        action: SemanticAction::Submit {
                            message: request.message.clone(),
                        },
                        expected_state: None,
                    },
                    &classification,
                    driver.as_ref(),
                )
                .map_err(map_driver_error)?;
            let codex = CodexDriver;
            match codex.begin_submission(&operation_id, &request.message, &snapshot) {
                Ok((submission, actions)) => {
                    if let Err(error) = apply_actions(session, &actions) {
                        let _ = session.actions.clear_active(&operation_id);
                        return Err(error);
                    }
                    drop(state);
                    self.drive_codex_submission(request, operation_id, submission)
                }
                Err(error) => {
                    let _ = session.actions.clear_active(&operation_id);
                    Err(map_driver_error(error))
                }
            }
        })
    }

    fn drive_codex_submission(
        &self,
        request: SessionSubmit,
        operation_id: String,
        mut submission: crate::codex::CodexSubmission,
    ) -> std::result::Result<SemanticOutcome, SessionError> {
        let deadline = Instant::now() + Duration::from_millis(request.timeout_ms);
        let codex = CodexDriver;
        let progress = loop {
            let snapshot = self.snapshot(SessionSelector {
                session_id: request.session_id.clone(),
            })?;
            let progress = codex.advance_submission(&mut submission, &snapshot);
            if !progress.actions.is_empty()
                && let Err(error) =
                    self.apply_actions_unlocked(&request.session_id, &progress.actions)
            {
                self.clear_action(&request.session_id, &operation_id);
                return Err(error);
            }
            match progress.phase {
                SubmissionPhase::Running
                | SubmissionPhase::Completed
                | SubmissionPhase::Failed
                | SubmissionPhase::Uncertain => break progress,
                SubmissionPhase::AwaitingComposer | SubmissionPhase::AwaitingOutcome => {
                    if Instant::now() >= deadline {
                        let snapshot = self.snapshot(SessionSelector {
                            session_id: request.session_id.clone(),
                        })?;
                        break codex.expire_submission(&mut submission, &snapshot);
                    }
                    std::thread::sleep(SEMANTIC_POLL);
                }
            }
        };

        self.finish_semantic(
            &request.session_id,
            &operation_id,
            &progress.classification,
            Some(phase_name(&progress.phase)),
            None,
        )
    }

    pub(in crate::daemon) fn interrupt(
        &self,
        request: SessionInterrupt,
        operation_id: String,
    ) -> std::result::Result<SemanticOutcome, SessionError> {
        validate_operation_id(&operation_id).map_err(SessionError::InvalidInput)?;
        let session_id = request.session_id.clone();
        self.with_session_gate(session_id, move |_session_id| {
            let mut state = self
                .state
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            let session = state.sessions.get_mut(&request.session_id).ok_or_else(|| {
                SessionError::NotFound(format!("unknown session: {}", request.session_id))
            })?;
            ensure_running(session)?;
            let driver = self
                .drivers
                .get(&session.info.driver)
                .map_err(map_driver_error)?;
            let classification = driver.classify(&session.snapshot());
            let planned = match session.actions.start(
                ActionRequest {
                    operation_id: operation_id.clone(),
                    action: SemanticAction::Interrupt,
                    expected_state: None,
                },
                &classification,
                driver.as_ref(),
            ) {
                Ok(planned) => planned,
                Err(error) => return Err(map_driver_error(error)),
            };
            if let Err(error) = apply_actions(session, &planned.actions) {
                let _ = session.actions.clear_active(&operation_id);
                return Err(error);
            }
            let classification = driver.classify(&session.snapshot());
            let outcome = session
                .actions
                .complete(&operation_id, &classification)
                .map_err(map_driver_error)?;
            Ok(semantic_outcome(
                &request.session_id,
                outcome.operation_id,
                &outcome.state,
                &outcome.evidence,
                None,
                None,
            ))
        })
    }

    pub(in crate::daemon) fn restart(
        &self,
        request: SessionRestart,
        operation_id: String,
    ) -> std::result::Result<SemanticOutcome, SessionError> {
        validate_operation_id(&operation_id).map_err(SessionError::InvalidInput)?;
        let session_id = request.session_id.clone();
        self.with_session_gate(session_id, move |_session_id| {
            let mut state = self
                .state
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            let session = state.sessions.get_mut(&request.session_id).ok_or_else(|| {
                SessionError::NotFound(format!("unknown session: {}", request.session_id))
            })?;
            session
                .actions
                .begin_exclusive(&operation_id)
                .map_err(map_driver_error)?;
            let start = StartSession {
                session_id: session.info.session_id.clone(),
                driver: session.info.driver.clone(),
                command: session.info.command.clone(),
                cwd: session.info.cwd.clone(),
                columns: session.snapshot().columns,
                rows: session.snapshot().rows,
            };
            let old_group = session.process_group;
            if let Err(error) = session.stop() {
                let _ = session.actions.clear_active(&operation_id);
                return Err(error.into());
            }
            if super::super::session::process_group_exists(old_group) {
                let _ = session.actions.clear_active(&operation_id);
                return Err(SessionError::Internal(anyhow::anyhow!(
                    "process group {old_group} still alive after restart stop"
                )));
            }
            let stopped = session.info.clone();
            publish_lifecycle(&self.events, &stopped);

            let mut replacement = match ManagedSession::spawn(start) {
                Ok(session) => session,
                Err(error) => {
                    let _ = session.actions.clear_active(&operation_id);
                    return Err(SessionError::Internal(error));
                }
            };
            if let Err(error) = replacement.start_output_drain(std::sync::Arc::clone(&self.events))
            {
                replacement.kill_group_best_effort();
                let _ = session.actions.clear_active(&operation_id);
                return Err(SessionError::Internal(error));
            }
            let info = replacement.info.clone();
            let driver = match self.drivers.get(&info.driver) {
                Ok(driver) => driver,
                Err(error) => {
                    replacement.kill_group_best_effort();
                    let _ = session.actions.clear_active(&operation_id);
                    return Err(map_driver_error(error));
                }
            };
            let classification = driver.classify(&replacement.snapshot());
            state.sessions.insert(info.session_id.clone(), replacement);
            publish_lifecycle(&self.events, &info);

            Ok(semantic_outcome(
                &info.session_id,
                operation_id,
                &classification.state,
                &classification.evidence,
                None,
                Some(info.clone()),
            ))
        })
    }

    fn apply_actions_unlocked(
        &self,
        session_id: &str,
        actions: &[TerminalAction],
    ) -> std::result::Result<(), SessionError> {
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let session = state
            .sessions
            .get_mut(session_id)
            .ok_or_else(|| SessionError::NotFound(format!("unknown session: {session_id}")))?;
        apply_actions(session, actions)
    }

    fn clear_action(&self, session_id: &str, operation_id: &str) {
        let state = self
            .state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if let Some(session) = state.sessions.get(session_id) {
            let _ = session.actions.clear_active(operation_id);
        }
    }

    fn finish_semantic(
        &self,
        session_id: &str,
        operation_id: &str,
        classification: &Classification,
        phase: Option<&str>,
        session: Option<SessionInfo>,
    ) -> std::result::Result<SemanticOutcome, SessionError> {
        let state = self
            .state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let managed = state
            .sessions
            .get(session_id)
            .ok_or_else(|| SessionError::NotFound(format!("unknown session: {session_id}")))?;
        let outcome = managed
            .actions
            .complete(operation_id, classification)
            .map_err(map_driver_error)?;
        Ok(semantic_outcome(
            session_id,
            outcome.operation_id,
            &outcome.state,
            &outcome.evidence,
            phase,
            session,
        ))
    }
}

fn ensure_running(session: &ManagedSession) -> Result<(), SessionError> {
    if !matches!(session.info.state, SessionState::Running) {
        return Err(SessionError::InvalidState(format!(
            "session is not running: {}",
            session.info.session_id
        )));
    }
    Ok(())
}

fn apply_actions(
    session: &mut ManagedSession,
    actions: &[TerminalAction],
) -> Result<(), SessionError> {
    for action in actions {
        match action {
            TerminalAction::WriteText(text) => {
                session.write_running(text.as_bytes())?;
            }
            TerminalAction::WriteBytes(bytes) => {
                session.write_running(bytes)?;
            }
            TerminalAction::Key { key, modifiers } => {
                let bytes = encode_key(key, modifiers).map_err(SessionError::InvalidInput)?;
                session.write_running(&bytes)?;
            }
            TerminalAction::WaitForState(_) | TerminalAction::WaitForScreenSettle => {}
        }
    }
    Ok(())
}

fn map_driver_error(error: DriverError) -> SessionError {
    match error {
        DriverError::Busy(operation_id) => {
            SessionError::Conflict(format!("session is busy with {operation_id}"))
        }
        DriverError::InvalidOperationId(message) | DriverError::UnknownDriver(message) => {
            SessionError::InvalidInput(message)
        }
        DriverError::Unsupported { .. }
        | DriverError::StateMismatch { .. }
        | DriverError::UnknownOperation(_) => SessionError::InvalidState(error.to_string()),
    }
}

fn semantic_outcome(
    session_id: &str,
    operation_id: String,
    state: &HarnessState,
    evidence: &[Evidence],
    phase: Option<&str>,
    session: Option<SessionInfo>,
) -> SemanticOutcome {
    SemanticOutcome {
        session_id: session_id.into(),
        operation_id,
        state: harness_state_name(state),
        evidence: evidence
            .iter()
            .map(|item| SemanticEvidence {
                source: item.source.clone(),
                detail: item.detail.clone(),
            })
            .collect(),
        phase: phase.map(str::to_owned),
        session,
    }
}

fn harness_state_name(state: &HarnessState) -> String {
    match state {
        HarnessState::Starting => "starting",
        HarnessState::WaitingForInput => "waiting_for_input",
        HarnessState::Submitting => "submitting",
        HarnessState::Running => "running",
        HarnessState::ApprovalRequired => "approval_required",
        HarnessState::Completed => "completed",
        HarnessState::Failed => "failed",
        HarnessState::Stopped => "stopped",
        HarnessState::Unknown => "unknown",
    }
    .into()
}

fn phase_name(phase: &SubmissionPhase) -> &'static str {
    match phase {
        SubmissionPhase::AwaitingComposer => "awaiting_composer",
        SubmissionPhase::AwaitingOutcome => "awaiting_outcome",
        SubmissionPhase::Running => "running",
        SubmissionPhase::Completed => "completed",
        SubmissionPhase::Failed => "failed",
        SubmissionPhase::Uncertain => "uncertain",
    }
}

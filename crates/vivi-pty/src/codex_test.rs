use super::*;
use crate::driver::{HarnessDriver, HarnessState, TerminalAction};
use crate::protocol::{TerminalModes, TerminalSnapshot};

fn snapshot(contents: &str, screen_revision: u64) -> TerminalSnapshot {
    TerminalSnapshot {
        session_id: "codex-test".into(),
        columns: 80,
        rows: 24,
        cursor_column: 0,
        cursor_row: 0,
        contents: contents.into(),
        formatted_contents: Vec::new(),
        scrollback: 0,
        scrollback_limit: 2_000,
        modes: TerminalModes {
            alternate_screen: false,
            application_keypad: false,
            application_cursor: false,
            cursor_hidden: false,
            bracketed_paste: false,
            mouse_protocol: "none".into(),
            mouse_encoding: "default".into(),
        },
        screen_revision,
        output_sequence: screen_revision,
    }
}

#[test]
fn codex_classification_is_conservative_across_known_states() {
    let driver = CodexDriver;
    let cases = [
        ("", HarnessState::Starting),
        ("› ", HarnessState::WaitingForInput),
        ("Allow this command? (y/n)", HarnessState::ApprovalRequired),
        ("Thinking…", HarnessState::Running),
        ("Task completed", HarnessState::Completed),
        ("error: command failed", HarnessState::Failed),
        ("unrecognized screen", HarnessState::Unknown),
    ];
    for (contents, expected) in cases {
        assert_eq!(driver.classify(&snapshot(contents, 1)).state, expected);
    }
}

#[test]
fn codex_plans_semantic_keys_and_rejects_process_restart() {
    let driver = CodexDriver;
    assert!(matches!(
        driver.plan(&SemanticAction::Approve),
        Ok(actions) if actions == vec![
            TerminalAction::Key { key: "y".into(), modifiers: Vec::new() },
            TerminalAction::Key { key: "Enter".into(), modifiers: Vec::new() },
        ]
    ));
    assert!(matches!(
        driver.plan(&SemanticAction::Reject),
        Ok(actions) if actions.len() == 2
    ));
    assert!(matches!(
        driver.plan(&SemanticAction::Restart),
        Err(DriverError::Unsupported { .. })
    ));
}

#[test]
fn submission_emits_enter_only_after_visible_composer_receipt() {
    let driver = CodexDriver;
    let initial = snapshot("› ", 10);
    let (mut submission, initial_actions) = driver
        .begin_submission("turn-1", "hello", &initial)
        .unwrap();
    assert_eq!(submission.phase, SubmissionPhase::AwaitingComposer);
    assert!(
        initial_actions
            .iter()
            .all(|action| !matches!(action, TerminalAction::Key { key, .. } if key == "Enter"))
    );

    let unchanged = driver.advance_submission(&mut submission, &initial);
    assert_eq!(unchanged.phase, SubmissionPhase::AwaitingComposer);
    assert!(unchanged.actions.is_empty());

    let received = snapshot("› hello", 11);
    let receipt = driver.advance_submission(&mut submission, &received);
    assert_eq!(receipt.phase, SubmissionPhase::AwaitingOutcome);
    assert!(
        receipt
            .actions
            .iter()
            .any(|action| matches!(action, TerminalAction::Key { key, .. } if key == "Enter"))
    );

    let running = driver.advance_submission(&mut submission, &snapshot("Thinking…", 12));
    assert_eq!(running.phase, SubmissionPhase::Running);
}

#[test]
fn submission_preserves_uncertainty_for_stale_or_contradictory_evidence() {
    let driver = CodexDriver;
    let initial = snapshot("› ", 20);
    let (mut stale, _) = driver
        .begin_submission("turn-stale", "hello", &initial)
        .unwrap();
    let expired = driver.expire_submission(&mut stale, &initial);
    assert_eq!(expired.phase, SubmissionPhase::Uncertain);

    let (mut contradictory, _) = driver
        .begin_submission("turn-contradictory", "hello", &initial)
        .unwrap();
    let progress = driver.advance_submission(&mut contradictory, &snapshot("different screen", 21));
    assert_eq!(progress.phase, SubmissionPhase::Uncertain);
    assert!(progress.actions.is_empty());
}

#[test]
fn submission_accepts_completed_and_failed_outcomes() {
    let driver = CodexDriver;
    for (operation_id, outcome, expected) in [
        (
            "turn-completed",
            "Task completed",
            SubmissionPhase::Completed,
        ),
        (
            "turn-failed",
            "error: command failed",
            SubmissionPhase::Failed,
        ),
    ] {
        let initial = snapshot("› ", 30);
        let (mut submission, _) = driver
            .begin_submission(operation_id, "hello", &initial)
            .unwrap();
        driver.advance_submission(&mut submission, &snapshot("› hello", 31));
        let progress = driver.advance_submission(&mut submission, &snapshot(outcome, 32));
        assert_eq!(progress.phase, expected);
    }
}

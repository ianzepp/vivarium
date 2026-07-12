use super::*;
use crate::driver::{HarnessDriver, HarnessState, SemanticAction, TerminalAction};
use crate::protocol::{TerminalModes, TerminalSnapshot};

fn snapshot(contents: &str) -> TerminalSnapshot {
    TerminalSnapshot {
        session_id: "opencode-test".into(),
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
        screen_revision: 1,
        output_sequence: 1,
    }
}

#[test]
fn opencode_uses_status_and_trust_markers_conservatively() {
    let driver = OpenCodeDriver;
    for (screen, state) in [
        ("Ask anything...", HarnessState::WaitingForInput),
        ("Build · esc interrupt", HarnessState::Running),
        (
            "Always allow until OpenCode is restarted",
            HarnessState::ApprovalRequired,
        ),
        ("Idle until next wake", HarnessState::Completed),
        ("stream timed out", HarnessState::Failed),
        ("codex ›", HarnessState::Unknown),
    ] {
        let classification = driver.classify(&snapshot(screen));
        assert_eq!(classification.state, state);
        assert!(!classification.evidence.is_empty());
    }
}

#[test]
fn opencode_plans_actions_and_rejects_restart() {
    let driver = OpenCodeDriver;
    let submit = driver
        .plan(&SemanticAction::Submit {
            message: "hello".into(),
        })
        .unwrap();
    assert!(matches!(submit[0], TerminalAction::WriteText(_)));
    assert!(driver.plan(&SemanticAction::Interrupt).is_ok());
    assert!(driver.plan(&SemanticAction::Approve).is_ok());
    assert!(driver.plan(&SemanticAction::Reject).is_ok());
    assert!(matches!(
        driver.plan(&SemanticAction::Restart),
        Err(crate::driver::DriverError::Unsupported { .. })
    ));
}

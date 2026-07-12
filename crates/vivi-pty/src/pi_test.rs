use super::*;
use crate::driver::{HarnessDriver, HarnessState, SemanticAction, TerminalAction};
use crate::protocol::{TerminalModes, TerminalSnapshot};

fn snapshot(contents: &str) -> TerminalSnapshot {
    TerminalSnapshot {
        session_id: "pi-test".into(),
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
fn pi_uses_only_pi_markers_and_preserves_unknown() {
    let driver = PiDriver;
    for (screen, state) in [
        ("❯", HarnessState::WaitingForInput),
        ("Working (esc to interrupt)", HarnessState::Running),
        (
            "Do you trust this workspace?",
            HarnessState::ApprovalRequired,
        ),
        ("Turn completed", HarnessState::Completed),
        ("connection failed", HarnessState::Failed),
        ("codex ›", HarnessState::Unknown),
    ] {
        let classification = driver.classify(&snapshot(screen));
        assert_eq!(classification.state, state);
        assert!(!classification.evidence.is_empty());
    }
}

#[test]
fn pi_plans_submit_interrupt_approval_and_reject() {
    let driver = PiDriver;
    let submit = driver
        .plan(&SemanticAction::Submit {
            message: "hello".into(),
        })
        .unwrap();
    assert!(matches!(submit[0], TerminalAction::WriteText(_)));
    assert!(
        submit
            .iter()
            .any(|action| matches!(action, TerminalAction::WaitForState(HarnessState::Running)))
    );
    assert!(driver.plan(&SemanticAction::Interrupt).is_ok());
    assert!(driver.plan(&SemanticAction::Approve).is_ok());
    assert!(driver.plan(&SemanticAction::Reject).is_ok());
}

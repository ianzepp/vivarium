use crate::driver::{Confidence, HarnessDriver, HarnessState, SemanticAction, TerminalAction};
use crate::grok::GrokDriver;
use crate::protocol::{TerminalModes, TerminalSnapshot};

fn snapshot(contents: impl Into<String>) -> TerminalSnapshot {
    TerminalSnapshot {
        session_id: "grok-test".into(),
        columns: 80,
        rows: 24,
        cursor_column: 0,
        cursor_row: 0,
        contents: contents.into(),
        formatted_contents: Vec::new(),
        scrollback: 0,
        scrollback_limit: 1000,
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
fn name_is_grok() {
    assert_eq!(GrokDriver.name(), "grok");
}

#[test]
fn capabilities_include_submit_interrupt_and_raw_input() {
    let capabilities = GrokDriver.capabilities();
    assert!(capabilities.submit);
    assert!(capabilities.interrupt);
    assert!(capabilities.raw_input);
    assert!(!capabilities.approve);
    assert!(!capabilities.reject);
    assert!(!capabilities.restart);
}

#[test]
fn empty_screen_classifies_as_starting() {
    let classification = GrokDriver.classify(&snapshot(""));
    assert_eq!(classification.state, HarnessState::Starting);
}

#[test]
fn shell_prompt_classifies_as_waiting_for_input() {
    let classification = GrokDriver.classify(&snapshot("last\n$ "));
    assert_eq!(classification.state, HarnessState::WaitingForInput);
}

#[test]
fn angle_bracket_prompt_classifies_as_waiting_for_input() {
    let classification = GrokDriver.classify(&snapshot("> "));
    assert_eq!(classification.state, HarnessState::WaitingForInput);
}

#[test]
fn grok_tui_prompt_classifies_as_waiting_for_input() {
    let classification =
        GrokDriver.classify(&snapshot("Grok Build Beta\nTip: Press Ctrl+O\n\n  ❯"));
    assert_eq!(classification.state, HarnessState::WaitingForInput);
}

#[test]
fn grok_tui_prompt_above_footer_classifies_as_waiting_for_input() {
    let classification = GrokDriver.classify(&snapshot(
        "Grok Build\n  │ ❯                                      │\n  ╰──── GPT-5.6 Terra · always-approve ────╯\n\n[stable]",
    ));
    assert_eq!(classification.state, HarnessState::WaitingForInput);
}

#[test]
fn completed_turn_prompt_without_grok_branding_is_waiting() {
    let classification = GrokDriver.classify(&snapshot(
        "Turn completed in 30s.\n\n  │ ❯                         │\n  ╰──── GPT-5.6 Sol ──────────╯\nShift+Tab:mode │ Ctrl+.:shortcuts",
    ));
    assert_eq!(classification.state, HarnessState::WaitingForInput);
}

#[test]
fn active_work_markers_outrank_the_visible_composer_prompt() {
    for active_marker in [
        "Thinking…",
        "Running tool: cargo test",
        "Background task: warmup",
    ] {
        let classification = GrokDriver.classify(&snapshot(format!(
            "Grok Build\n{active_marker}\n\n  │ ❯                         │\n  ╰──── GPT-5.6 Sol ──────────╯"
        )));
        assert_eq!(classification.state, HarnessState::Running);
        assert_eq!(classification.confidence, Confidence::High);
    }
}

#[test]
fn grok_tui_working_classifies_as_running() {
    let classification = GrokDriver.classify(&snapshot("Grok Build Beta\nWorking on it…"));
    assert_eq!(classification.state, HarnessState::Running);
}

#[test]
fn capacity_error_classifies_as_failed() {
    let classification = GrokDriver.classify(&snapshot("rate limit exceeded, try again"));
    assert_eq!(classification.state, HarnessState::Failed);
}

#[test]
fn visible_output_without_prompt_classifies_as_running() {
    let classification = GrokDriver.classify(&snapshot("Working on it…\nSome output"));
    assert_eq!(classification.state, HarnessState::Running);
}

#[test]
fn submit_plan_writes_text_and_presses_enter() {
    let actions = GrokDriver
        .plan(&SemanticAction::Submit {
            message: "hello".into(),
        })
        .unwrap();
    assert_eq!(actions.len(), 3);
    assert!(matches!(&actions[0], TerminalAction::WriteText(text) if text == "hello"));
    assert!(
        matches!(&actions[1], TerminalAction::Key { key, modifiers } if key == "Enter" && modifiers.is_empty())
    );
    assert!(matches!(
        &actions[2],
        TerminalAction::WaitForState(HarnessState::Running)
    ));
}

#[test]
fn interrupt_plan_sends_control_c() {
    let actions = GrokDriver.plan(&SemanticAction::Interrupt).unwrap();
    assert_eq!(actions.len(), 1);
    assert!(
        matches!(&actions[0], TerminalAction::Key { key, modifiers } if key == "c" && modifiers.len() == 1)
    );
}

#[test]
fn approve_and_reject_are_unsupported() {
    assert!(GrokDriver.plan(&SemanticAction::Approve).is_err());
    assert!(GrokDriver.plan(&SemanticAction::Reject).is_err());
    assert!(GrokDriver.plan(&SemanticAction::Restart).is_err());
}

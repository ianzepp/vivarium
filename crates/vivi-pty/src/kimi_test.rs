use crate::driver::{Confidence, HarnessDriver, HarnessState, SemanticAction, TerminalAction};
use crate::kimi::KimiDriver;
use crate::protocol::{TerminalModes, TerminalSnapshot};

fn snapshot(contents: impl Into<String>) -> TerminalSnapshot {
    TerminalSnapshot {
        session_id: "kimi-test".into(),
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
fn name_is_kimi() {
    assert_eq!(KimiDriver.name(), "kimi");
}

#[test]
fn capabilities_include_submit_interrupt_approve_reject_and_raw_input() {
    let capabilities = KimiDriver.capabilities();
    assert!(capabilities.submit);
    assert!(capabilities.interrupt);
    assert!(capabilities.approve);
    assert!(capabilities.reject);
    assert!(capabilities.raw_input);
    assert!(!capabilities.restart);
}

#[test]
fn empty_screen_classifies_as_starting() {
    let classification = KimiDriver.classify(&snapshot(""));
    assert_eq!(classification.state, HarnessState::Starting);
}

#[test]
fn idle_composer_classifies_as_waiting_for_input() {
    let classification = KimiDriver.classify(&snapshot(
        "│  ▐█▛█▛█▌  Welcome to Kimi Code!  │\n\
         ╰────────────────────────────────╯\n\
         ╭────────────────────────────────╮\n\
         │ >                              │\n\
         ╰────────────────────────────────╯\n\
         yolo  K3 thinking: high  …/work            context: 0% (0/256k)",
    ));
    assert_eq!(classification.state, HarnessState::WaitingForInput);
    assert_eq!(classification.confidence, Confidence::High);
}

#[test]
fn composer_with_unsubmitted_text_classifies_as_waiting_for_input() {
    let classification = KimiDriver.classify(&snapshot(
        "╭────────────────────────────────╮\n\
         │ > Reply with exactly: OK       │\n\
         ╰────────────────────────────────╯\n\
         K3 thinking: high  …/work                context: 0% (0/256k)",
    ));
    assert_eq!(classification.state, HarnessState::WaitingForInput);
}

#[test]
fn shell_mode_composer_classifies_as_waiting_for_input() {
    let classification = KimiDriver.classify(&snapshot(
        "╭────────────────────────────────╮\n\
         │ ! ls -la                     │\n\
         ╰────────────────────────────────╯\n\
         K3 thinking: high  …/work                context: 9% (22.6k/256k)",
    ));
    assert_eq!(classification.state, HarnessState::WaitingForInput);
}

#[test]
fn turn_spinner_outranks_the_visible_composer() {
    for glyph in ['🌕', '🌒', '🌗', '🌓'] {
        let classification = KimiDriver.classify(&snapshot(format!(
            "✨ Reply with exactly: OK\n\n\
             {glyph} · Tip: /plugins: manage plugins\n\
             ╭────────────────────────────────╮\n\
             │ >                              │\n\
             ╰────────────────────────────────╯\n\
             K3 thinking: high  …/work            context: 0% (0/256k)"
        )));
        assert_eq!(classification.state, HarnessState::Running);
        assert_eq!(classification.confidence, Confidence::High);
    }
}

#[test]
fn approval_panel_classifies_as_approval_required() {
    let classification = KimiDriver.classify(&snapshot(
        "▶ Run this command?\n\n\
         cwd: /work\n\
         $ pwd\n\n\
         ▶ 1. Approve once\n\
         ␣ 2. Approve for this session\n\
         ␣ 3. Reject\n\
         ␣ 4. Reject with feedback\n\n\
         ↑/↓ select · 1/2/3/4 choose · ↵ confirm\n\
         K3 thinking: high  …/work                context: 9% (22.6k/256k)",
    ));
    assert_eq!(classification.state, HarnessState::ApprovalRequired);
    assert_eq!(classification.confidence, Confidence::High);
}

#[test]
fn interrupted_turn_returns_to_waiting_for_input() {
    let classification = KimiDriver.classify(&snapshot(
        "✨ Count slowly from 1 to 50\n\n\
         ● The user asks to count slowly...\n\
         Interrupted by user\n\n\
         ╭────────────────────────────────╮\n\
         │ >                              │\n\
         ╰────────────────────────────────╯\n\
         K3 thinking: high  …/work                context: 9% (22.7k/256k)",
    ));
    assert_eq!(classification.state, HarnessState::WaitingForInput);
}

#[test]
fn capacity_error_classifies_as_failed() {
    let classification = KimiDriver.classify(&snapshot("rate limit exceeded, try again"));
    assert_eq!(classification.state, HarnessState::Failed);
}

#[test]
fn tui_without_clear_prompt_classifies_as_running() {
    let classification = KimiDriver.classify(&snapshot(
        "● OK\nK3 thinking: high  …/work                context: 9% (22.6k/256k)",
    ));
    assert_eq!(classification.state, HarnessState::Running);
    assert_eq!(classification.confidence, Confidence::Medium);
}

#[test]
fn shell_prompt_classifies_as_waiting_for_input() {
    let classification = KimiDriver.classify(&snapshot("last\n$ "));
    assert_eq!(classification.state, HarnessState::WaitingForInput);
}

#[test]
fn unrecognized_screen_classifies_as_unknown() {
    let classification = KimiDriver.classify(&snapshot("some unrelated output"));
    assert_eq!(classification.state, HarnessState::Unknown);
    assert_eq!(classification.confidence, Confidence::Low);
}

#[test]
fn submit_plan_writes_settles_then_presses_enter() {
    let actions = KimiDriver
        .plan(&SemanticAction::Submit {
            message: "hello".into(),
        })
        .unwrap();
    assert_eq!(actions.len(), 4);
    assert!(matches!(&actions[0], TerminalAction::WriteText(text) if text == "hello"));
    assert!(matches!(&actions[1], TerminalAction::WaitForScreenSettle));
    assert!(
        matches!(&actions[2], TerminalAction::Key { key, modifiers } if key == "Enter" && modifiers.is_empty())
    );
    assert!(matches!(
        &actions[3],
        TerminalAction::WaitForState(HarnessState::Running)
    ));
}

#[test]
fn interrupt_plan_sends_escape_not_ctrl_c() {
    let actions = KimiDriver.plan(&SemanticAction::Interrupt).unwrap();
    assert_eq!(actions.len(), 1);
    assert!(
        matches!(&actions[0], TerminalAction::Key { key, modifiers } if key == "Escape" && modifiers.is_empty())
    );
}

#[test]
fn approve_plan_confirms_and_reject_plan_sends_escape() {
    let approve = KimiDriver.plan(&SemanticAction::Approve).unwrap();
    assert!(matches!(&approve[0], TerminalAction::Key { key, .. } if key == "Enter"));
    let reject = KimiDriver.plan(&SemanticAction::Reject).unwrap();
    assert!(matches!(&reject[0], TerminalAction::Key { key, .. } if key == "Escape"));
}

#[test]
fn restart_is_unsupported() {
    assert!(KimiDriver.plan(&SemanticAction::Restart).is_err());
}

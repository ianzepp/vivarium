use crate::driver::{HarnessDriver, SemanticAction, TerminalAction};
use crate::protocol::{TerminalModes, TerminalSnapshot};

fn snapshot(contents: &str) -> TerminalSnapshot {
    TerminalSnapshot {
        session_id: "conformance".into(),
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

fn assert_conforms(driver: &dyn HarnessDriver) {
    for screen in ["", "unknown evidence", "working"] {
        let classification = driver.classify(&snapshot(screen));
        assert!(
            !classification.evidence.is_empty(),
            "{} has no evidence",
            driver.name()
        );
    }
    assert!(driver.capabilities().raw_input);
    let raw = driver
        .plan(&SemanticAction::Raw(TerminalAction::WaitForScreenSettle))
        .unwrap();
    assert_eq!(raw, vec![TerminalAction::WaitForScreenSettle]);
}

#[test]
fn built_in_drivers_share_the_normalized_conformance_surface() {
    let drivers: Vec<Box<dyn HarnessDriver>> = vec![
        Box::new(crate::driver::GenericDriver),
        Box::new(crate::codex::CodexDriver),
        Box::new(crate::grok::GrokDriver),
        Box::new(crate::kimi::KimiDriver),
        Box::new(crate::pi::PiDriver),
        Box::new(crate::opencode::OpenCodeDriver),
    ];
    for driver in drivers {
        assert_conforms(driver.as_ref());
    }
}

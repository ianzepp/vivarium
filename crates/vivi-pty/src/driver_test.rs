use super::*;

fn snapshot(contents: &str) -> TerminalSnapshot {
    TerminalSnapshot {
        session_id: "driver-test".into(),
        columns: 80,
        rows: 24,
        cursor_column: 0,
        cursor_row: 0,
        contents: contents.into(),
        formatted_contents: Vec::new(),
        scrollback: 0,
        scrollback_limit: 2_000,
        modes: crate::protocol::TerminalModes {
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
fn generic_driver_classifies_prompt_output_and_ambiguity() {
    let driver = GenericDriver;
    assert_eq!(
        driver.classify(&snapshot("agent$ ")).state,
        HarnessState::WaitingForInput
    );
    assert_eq!(
        driver.classify(&snapshot("working")).state,
        HarnessState::Running
    );
    assert_eq!(
        driver.classify(&snapshot("   ")).state,
        HarnessState::Unknown
    );
}

#[test]
fn generic_driver_plans_guarded_actions_and_rejects_unsupported() {
    let driver = GenericDriver;
    let classification = driver.classify(&snapshot("agent$ "));
    let queue = ActionQueue::default();
    let planned = queue
        .start(
            ActionRequest {
                operation_id: "submit-1".into(),
                action: SemanticAction::Submit {
                    message: "hello".into(),
                },
                expected_state: Some(HarnessState::WaitingForInput),
            },
            &classification,
            &driver,
        )
        .unwrap();
    assert_eq!(planned.actions.len(), 3);
    assert!(matches!(
        queue.start(
            ActionRequest {
                operation_id: "submit-2".into(),
                action: SemanticAction::Interrupt,
                expected_state: None,
            },
            &classification,
            &driver,
        ),
        Err(DriverError::Busy(_))
    ));
    let outcome = queue.complete("submit-1", &classification).unwrap();
    assert_eq!(outcome.state, HarnessState::WaitingForInput);
    assert!(matches!(
        queue.start(
            ActionRequest {
                operation_id: "approve-1".into(),
                action: SemanticAction::Approve,
                expected_state: None,
            },
            &classification,
            &driver,
        ),
        Err(DriverError::Unsupported { .. })
    ));
    assert!(matches!(
        driver.plan(&SemanticAction::Approve),
        Err(DriverError::Unsupported { .. })
    ));
}

#[test]
fn registry_and_state_guards_are_explicit() {
    let registry = DriverRegistry::with_generic();
    assert_eq!(registry.get("generic").unwrap().name(), "generic");
    assert!(matches!(
        registry.get("missing"),
        Err(DriverError::UnknownDriver(_))
    ));

    let queue = ActionQueue::default();
    let driver = GenericDriver;
    let classification = driver.classify(&snapshot("working"));
    let error = queue.start(
        ActionRequest {
            operation_id: "guarded".into(),
            action: SemanticAction::Submit {
                message: "hello".into(),
            },
            expected_state: Some(HarnessState::WaitingForInput),
        },
        &classification,
        &driver,
    );
    assert!(matches!(error, Err(DriverError::StateMismatch { .. })));
}

#[test]
fn normalized_states_have_stable_wire_names() {
    let states = [
        HarnessState::Starting,
        HarnessState::WaitingForInput,
        HarnessState::Submitting,
        HarnessState::Running,
        HarnessState::ApprovalRequired,
        HarnessState::Completed,
        HarnessState::Failed,
        HarnessState::Stopped,
        HarnessState::Unknown,
    ];
    let encoded = states
        .iter()
        .map(|state| serde_json::to_string(state).unwrap())
        .collect::<Vec<_>>();
    assert_eq!(
        encoded,
        [
            "\"starting\"",
            "\"waiting_for_input\"",
            "\"submitting\"",
            "\"running\"",
            "\"approval_required\"",
            "\"completed\"",
            "\"failed\"",
            "\"stopped\"",
            "\"unknown\"",
        ]
    );
}

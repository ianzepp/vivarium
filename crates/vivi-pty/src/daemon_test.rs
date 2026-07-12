use super::*;
use crate::driver::{Confidence, HarnessState};
use std::os::unix::net::UnixListener;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;

static PTY_TEST_LOCK: Mutex<()> = Mutex::new(());

fn lock_pty_tests() -> MutexGuard<'static, ()> {
    PTY_TEST_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn request(id: &str, command: &[&str]) -> StartSession {
    StartSession {
        session_id: id.into(),
        driver: "generic".into(),
        command: command.iter().map(|part| (*part).into()).collect(),
        cwd: "/tmp".into(),
        columns: 80,
        rows: 24,
    }
}

fn selector(id: &str) -> SessionSelector {
    SessionSelector {
        session_id: id.into(),
    }
}

fn wait_for_screen(registry: &SessionRegistry, id: &str, marker: &str) {
    let deadline = std::time::Instant::now() + Duration::from_secs(2);
    loop {
        let snapshot = registry.snapshot(selector(id)).unwrap();
        if snapshot.contents.contains(marker) {
            return;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "screen output timeout waiting for {marker:?}"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn registry_observes_process_exit() {
    let _lock = lock_pty_tests();
    let registry = SessionRegistry::default();
    registry
        .start(request("exit-test", &["/bin/sh", "-c", "exit 7"]))
        .unwrap();
    std::thread::sleep(Duration::from_millis(100));

    let session = registry.inspect(selector("exit-test")).unwrap();
    assert_eq!(session.state, SessionState::Exited);
    assert_eq!(session.exit_code, Some(7));
}

#[test]
fn registry_stops_running_process_group() {
    let _lock = lock_pty_tests();
    let registry = SessionRegistry::default();
    registry
        .start(request("stop-test", &["/bin/sh", "-c", "sleep 30"]))
        .unwrap();
    let process_group = registry
        .state
        .lock()
        .unwrap()
        .sessions
        .get("stop-test")
        .unwrap()
        .process_group;

    let session = registry.stop(selector("stop-test")).unwrap();
    assert_eq!(session.state, SessionState::Stopped);
    assert!(!process_group_exists(process_group));
    assert_eq!(
        registry.stop(selector("stop-test")).unwrap().state,
        SessionState::Stopped
    );
}

#[test]
fn stopping_a_session_removes_descendants_in_its_group() {
    let _lock = lock_pty_tests();
    let registry = SessionRegistry::default();
    registry
        .start(request(
            "descendant-test",
            &[
                "/bin/sh",
                "-c",
                "sleep 30 & child=$!; echo child:$child; wait",
            ],
        ))
        .unwrap();
    wait_for_screen(&registry, "descendant-test", "child:");
    let process_group = registry
        .state
        .lock()
        .unwrap()
        .sessions
        .get("descendant-test")
        .unwrap()
        .process_group;

    registry.stop(selector("descendant-test")).unwrap();
    assert!(!process_group_exists(process_group));
}

#[test]
fn registry_writes_command_and_reads_rendered_screen() {
    let _lock = lock_pty_tests();
    let registry = SessionRegistry::default();
    registry
        .start(request("screen-test", &["/bin/sh"]))
        .unwrap();
    registry
        .write(TerminalWrite {
            session_id: "screen-test".into(),
            data: "printf 'vivi-pty-screen-ok\\n'\r".into(),
        })
        .unwrap();

    wait_for_screen(&registry, "screen-test", "vivi-pty-screen-ok");
}

#[test]
fn registry_writes_raw_bytes_without_text_decoding() {
    let _lock = lock_pty_tests();
    let registry = SessionRegistry::default();
    registry
        .start(request("raw-write-test", &["/bin/sh", "-c", "sleep 30"]))
        .unwrap();

    let written = registry
        .write_bytes(TerminalWriteBytes {
            session_id: "raw-write-test".into(),
            data: vec![0, 0xff, 0x1b, b'\n'],
        })
        .unwrap();
    assert_eq!(written, 4);
}

#[test]
fn resize_updates_pty_and_terminal_snapshot() {
    let _lock = lock_pty_tests();
    let registry = SessionRegistry::default();
    registry
        .start(request("resize-test", &["/bin/sh", "-c", "sleep 30"]))
        .unwrap();

    let snapshot = registry
        .resize(TerminalResize {
            session_id: "resize-test".into(),
            columns: 100,
            rows: 30,
        })
        .unwrap();
    assert_eq!((snapshot.columns, snapshot.rows), (100, 30));

    let state = registry.state.lock().unwrap();
    let size = state.sessions["resize-test"].pty_size().unwrap();
    assert_eq!((size.cols, size.rows), (100, 30));
}

#[test]
fn snapshot_reports_modes_revisions_and_formatted_contents() {
    let _lock = lock_pty_tests();
    let registry = SessionRegistry::default();
    let ansi = "\x1b[?1049h\x1b[2J\x1b[?25lalt-screen";
    let command = format!("printf '{}'; sleep 30", ansi);
    registry
        .start(request("snapshot-test", &["/bin/sh", "-c", &command]))
        .unwrap();
    let initial = registry.snapshot(selector("snapshot-test")).unwrap();
    wait_for_screen(&registry, "snapshot-test", "alt-screen");

    let snapshot = registry.snapshot(selector("snapshot-test")).unwrap();
    assert!(snapshot.modes.alternate_screen);
    assert!(snapshot.modes.cursor_hidden);
    assert!(snapshot.screen_revision > 0);
    assert!(snapshot.output_sequence > 0);
    assert!(snapshot.screen_revision >= initial.screen_revision);
    assert!(snapshot.output_sequence >= initial.output_sequence);
    assert!(!snapshot.formatted_contents.is_empty());
    assert_eq!(snapshot.scrollback_limit, MAX_SCROLLBACK_ROWS);
}

#[test]
fn high_output_keeps_bounded_scrollback_metadata() {
    let _lock = lock_pty_tests();
    let registry = SessionRegistry::default();
    let command =
        "i=0; while [ $i -lt 3000 ]; do printf 'line-%s\\n' $i; i=$((i+1)); done; sleep 30";
    registry
        .start(request("high-output-test", &["/bin/sh", "-c", command]))
        .unwrap();
    wait_for_screen(&registry, "high-output-test", "line-2999");

    let snapshot = registry.snapshot(selector("high-output-test")).unwrap();
    assert!(snapshot.scrollback <= snapshot.scrollback_limit);
    assert_eq!(snapshot.scrollback_limit, MAX_SCROLLBACK_ROWS);
}

#[test]
fn invalid_identifiers_and_duplicate_sessions_are_typed() {
    let _lock = lock_pty_tests();
    let registry = SessionRegistry::default();
    let invalid = registry.start(request("bad/id", &["/bin/sh"])).unwrap_err();
    assert!(matches!(invalid, SessionError::InvalidInput(_)));

    registry
        .start(request("duplicate-test", &["/bin/sh", "-c", "sleep 30"]))
        .unwrap();
    let duplicate = registry
        .start(request("duplicate-test", &["/bin/sh"]))
        .unwrap_err();
    assert!(matches!(duplicate, SessionError::Conflict(_)));
}

#[test]
fn full_registry_returns_a_resource_limit_without_tombstones() {
    let _lock = lock_pty_tests();
    let registry = SessionRegistry::default();
    for index in 0..MAX_SESSIONS {
        registry
            .start(request(
                &format!("capacity-{index}"),
                &["/bin/sh", "-c", "sleep 30"],
            ))
            .unwrap();
    }
    let error = registry
        .start(request("capacity-overflow", &["/bin/sh"]))
        .unwrap_err();
    assert!(matches!(error, SessionError::ResourceLimit(_)));
}

#[test]
fn exited_sessions_are_bounded_tombstones() {
    let _lock = lock_pty_tests();
    let registry = SessionRegistry::default();
    for index in 0..(MAX_TOMBSTONES + 3) {
        registry
            .start(request(
                &format!("tombstone-{index}"),
                &["/bin/sh", "-c", "exit 0"],
            ))
            .unwrap();
    }
    std::thread::sleep(Duration::from_millis(150));
    let sessions = registry.list().unwrap();
    assert!(sessions.len() <= MAX_TOMBSTONES);
}

#[test]
fn stopping_exited_sessions_frees_registry_capacity() {
    let _lock = lock_pty_tests();
    let registry = SessionRegistry::default();
    for index in 0..(MAX_SESSIONS + 1) {
        registry
            .start(request(
                &format!("stop-exited-{index}"),
                &["/bin/sh", "-c", "exit 0"],
            ))
            .unwrap();
        // Wait for the child to exit on its own, then stop it. The registry
        // must still treat this as a tombstone transition so the session slot
        // is freed for the next iteration.
        std::thread::sleep(Duration::from_millis(50));
        registry
            .stop(selector(&format!("stop-exited-{index}")))
            .unwrap();
    }
    // All stopped sessions should be evictable; the final start succeeded.
    let sessions = registry.list().unwrap();
    assert!(sessions.len() <= MAX_SESSIONS);
}

#[test]
fn multiple_sessions_can_be_started_and_inspected_concurrently() {
    let _lock = lock_pty_tests();
    let registry = Arc::new(SessionRegistry::default());
    let mut workers = Vec::new();
    for index in 0..8 {
        let registry = Arc::clone(&registry);
        workers.push(std::thread::spawn(move || {
            registry
                .start(request(
                    &format!("concurrent-{index}"),
                    &["/bin/sh", "-c", "sleep 1"],
                ))
                .unwrap();
            registry
                .inspect(selector(&format!("concurrent-{index}")))
                .unwrap();
        }));
    }
    for worker in workers {
        worker.join().unwrap();
    }
    assert_eq!(registry.list().unwrap().len(), 8);
}

#[test]
fn stale_socket_is_replaced_but_live_socket_is_rejected() {
    let path = std::env::temp_dir().join(format!("vivi-pty-test-{}", std::process::id()));
    let _ = std::fs::remove_file(&path);
    std::fs::write(&path, b"stale").unwrap();
    prepare_socket(&path).unwrap();
    assert!(!path.exists());

    let listener = UnixListener::bind(&path).unwrap();
    let error = prepare_socket(&path).unwrap_err();
    assert!(error.to_string().contains("daemon already listening"));
    drop(listener);
    std::fs::remove_file(path).unwrap();
}

#[test]
fn dispatch_exposes_typed_session_errors() {
    let registry = SessionRegistry::default();
    let response = dispatch(
        Request::new(
            1,
            "session.inspect",
            serde_json::json!({ "session_id": "missing" }),
        ),
        &registry,
    );
    assert_eq!(response.error.unwrap().code, error_codes::SESSION_NOT_FOUND);

    let response = dispatch(
        Request::new(
            2,
            "session.start",
            serde_json::json!({ "session_id": "bad/id" }),
        ),
        &registry,
    );
    assert_eq!(response.error.unwrap().code, error_codes::INVALID_PARAMS);

    let response = dispatch(
        Request::new(
            3,
            "terminal.resize",
            serde_json::json!({
                "session_id": "missing",
                "columns": 0,
                "rows": 24
            }),
        ),
        &registry,
    );
    assert_eq!(response.error.unwrap().code, error_codes::INVALID_PARAMS);

    let response = dispatch(
        Request::new(
            4,
            "terminal.key",
            serde_json::json!({
                "session_id": "missing",
                "key": "unknown"
            }),
        ),
        &registry,
    );
    assert_eq!(response.error.unwrap().code, error_codes::INVALID_PARAMS);
}

#[test]
fn attachment_is_read_only_and_control_writes_require_an_exclusive_lease() {
    let _lock = lock_pty_tests();
    let registry = SessionRegistry::default();
    registry
        .start(request("lease-test", &["/bin/sh", "-c", "sleep 30"]))
        .unwrap();

    let attached = dispatch(
        Request::new(
            6,
            "session.attach",
            serde_json::json!({ "session_id": "lease-test" }),
        ),
        &registry,
    );
    let attachment: AttachmentAck = serde_json::from_value(attached.result.unwrap()).unwrap();
    assert!(attachment.read_only);

    let lease_response = dispatch(
        Request::new(
            7,
            "session.lease.acquire",
            serde_json::json!({
                "session_id": "lease-test",
                "holder": "operator",
                "ttl_ms": 1_000
            }),
        ),
        &registry,
    );
    let lease: ControlLease = serde_json::from_value(lease_response.result.unwrap()).unwrap();

    let denied = dispatch(
        Request::new(
            8,
            "terminal.control_write",
            serde_json::json!({
                "session_id": "lease-test",
                "lease_id": "wrong",
                "data": "blocked"
            }),
        ),
        &registry,
    );
    assert_eq!(denied.error.unwrap().code, error_codes::LEASE_REQUIRED);

    let written = dispatch(
        Request::new(
            9,
            "terminal.control_write",
            serde_json::to_value(LeasedTerminalWrite {
                session_id: lease.session_id.clone(),
                lease_id: lease.lease_id.clone(),
                data: "accepted".into(),
            })
            .unwrap(),
        ),
        &registry,
    );
    assert_eq!(written.result.unwrap()["written"], 8);

    let second = dispatch(
        Request::new(
            10,
            "session.lease.acquire",
            serde_json::json!({
                "session_id": "lease-test",
                "holder": "other"
            }),
        ),
        &registry,
    );
    assert_eq!(second.error.unwrap().code, error_codes::LEASE_CONFLICT);

    let released = dispatch(
        Request::new(
            11,
            "session.lease.release",
            serde_json::to_value(SessionLeaseRelease {
                session_id: lease.session_id,
                lease_id: lease.lease_id,
            })
            .unwrap(),
        ),
        &registry,
    );
    assert!(released.error.is_none());
}

#[test]
fn diagnostic_snapshot_contains_protocol_process_and_terminal_evidence() {
    let _lock = lock_pty_tests();
    let registry = SessionRegistry::default();
    registry
        .start(request("diagnostic-test", &["/bin/sh", "-c", "sleep 30"]))
        .unwrap();

    let response = dispatch(
        Request::new(
            5,
            "session.diagnostic",
            serde_json::json!({ "session_id": "diagnostic-test" }),
        ),
        &registry,
    );
    let snapshot: DiagnosticSnapshot = serde_json::from_value(response.result.unwrap()).unwrap();
    assert_eq!(snapshot.protocol.protocol_version, PROTOCOL_VERSION);
    assert_eq!(snapshot.process_state, SessionState::Running);
    assert_eq!(snapshot.harness_state, HarnessState::Unknown);
    assert_eq!(snapshot.confidence, Confidence::Low);
    assert!(!snapshot.evidence.is_empty());
    assert_eq!(snapshot.session.session_id, "diagnostic-test");
    assert_eq!(snapshot.terminal.session_id, "diagnostic-test");
}

#[test]
fn operation_ids_replay_without_repeating_and_conflict_on_different_requests() {
    let _lock = lock_pty_tests();
    let registry = SessionRegistry::default();
    let params = serde_json::json!({
        "session_id": "operation-test",
        "driver": "generic",
        "command": ["/bin/sh", "-c", "sleep 30"],
        "cwd": "/tmp",
        "columns": 80,
        "rows": 24
    });
    let mut first = Request::new(1, "session.start", params.clone());
    first.operation_id = Some("operation-1".into());
    let first_response = dispatch(first, &registry);
    assert!(first_response.error.is_none());

    let mut retry = Request::new(2, "session.start", params);
    retry.operation_id = Some("operation-1".into());
    let retry_response = dispatch(retry, &registry);
    assert!(retry_response.error.is_none());
    assert_eq!(retry_response.operation_id.as_deref(), Some("operation-1"));
    assert_eq!(registry.list().unwrap().len(), 1);

    let mut conflict = Request::new(
        3,
        "session.start",
        serde_json::json!({
            "session_id": "operation-test",
            "command": ["/bin/sh"],
            "cwd": "/tmp",
            "columns": 80,
            "rows": 24
        }),
    );
    conflict.operation_id = Some("operation-1".into());
    let conflict_response = dispatch(conflict, &registry);
    assert_eq!(
        conflict_response.error.unwrap().code,
        error_codes::OPERATION_CONFLICT
    );
    let events = registry.events.batch("operation-test", 0);
    assert!(
        events
            .events
            .iter()
            .any(|event| { matches!(event.kind, SessionEventKind::Operation { .. }) })
    );
}

#[test]
fn wait_completes_and_times_out_with_typed_results() {
    let _lock = lock_pty_tests();
    let registry = SessionRegistry::default();
    registry
        .start(request("wait-exit", &["/bin/sh", "-c", "exit 0"]))
        .unwrap();
    let completed = registry
        .wait(SessionWait {
            session_id: "wait-exit".into(),
            state: Some(SessionState::Exited),
            screen_revision: None,
            event_sequence: None,
            timeout_ms: 1_000,
        })
        .unwrap();
    assert_eq!(completed.session.state, SessionState::Exited);

    registry
        .start(request("wait-timeout", &["/bin/sh", "-c", "sleep 30"]))
        .unwrap();
    let timeout = registry.wait(SessionWait {
        session_id: "wait-timeout".into(),
        state: Some(SessionState::Exited),
        screen_revision: None,
        event_sequence: None,
        timeout_ms: 0,
    });
    assert!(matches!(timeout, Err(SessionError::Timeout(_))));
}

#[test]
fn persistent_subscription_receives_framed_event_notifications() {
    let _lock = lock_pty_tests();
    let registry = Arc::new(SessionRegistry::default());
    registry
        .start(request("subscription-test", &["/bin/sh", "-c", "sleep 30"]))
        .unwrap();
    let (mut client, server) = std::os::unix::net::UnixStream::pair().unwrap();
    let worker_registry = Arc::clone(&registry);
    let worker = std::thread::spawn(move || serve_client(server, worker_registry));
    client
        .set_read_timeout(Some(Duration::from_secs(2)))
        .unwrap();
    write_frame(
        &mut client,
        &Request::new(
            1,
            "session.subscribe",
            serde_json::json!({
                "session_id": "subscription-test",
                "after_sequence": 0
            }),
        ),
    )
    .unwrap();
    let response: Response = read_frame(&mut client).unwrap();
    assert!(response.error.is_none());
    let notification: ServerNotification = read_frame(&mut client).unwrap();
    assert_eq!(notification.method, "session.event");
    let batch: EventBatch = serde_json::from_value(notification.params).unwrap();
    assert_eq!(batch.session_id, "subscription-test");
    assert!(!batch.events.is_empty());

    drop(client);
    worker.join().unwrap().unwrap();
}

#[test]
fn lagged_subscription_includes_current_diagnostic_snapshot() {
    let _lock = lock_pty_tests();
    let registry = SessionRegistry::default();
    registry
        .start(request("lag-test", &["/bin/sh", "-c", "sleep 30"]))
        .unwrap();
    for _ in 0..(MAX_EVENT_HISTORY + 2) {
        registry.events.publish(
            "lag-test",
            SessionEventKind::Screen {
                screen_revision: 1,
                output_sequence: 1,
            },
        );
    }

    let batch = registry.event_batch(selector("lag-test"), 0).unwrap();
    assert!(batch.lagged);
    assert!(batch.events.is_empty());
    assert!(batch.snapshot.is_some());
}

#[test]
fn semantic_busy_rejects_without_pty_input() {
    let _lock = lock_pty_tests();
    let registry = SessionRegistry::default();
    registry
        .start(request("busy-test", &["/bin/sh", "-c", "sleep 30"]))
        .unwrap();
    let process_group = registry
        .state
        .lock()
        .unwrap()
        .sessions
        .get("busy-test")
        .unwrap()
        .process_group;
    {
        let state = registry.state.lock().unwrap();
        let session = state.sessions.get("busy-test").unwrap();
        session.actions.begin_exclusive("hold-op").unwrap();
    }

    let mut interrupt = Request::new(
        1,
        "session.interrupt",
        serde_json::json!({ "session_id": "busy-test" }),
    );
    interrupt.operation_id = Some("interrupt-1".into());
    let response = dispatch(interrupt, &registry);
    assert_eq!(
        response.error.as_ref().map(|error| error.code),
        Some(error_codes::SESSION_CONFLICT)
    );
    assert!(response.error.unwrap().message.contains("busy"));
    assert!(process_group_exists(process_group));
    assert_eq!(
        registry.inspect(selector("busy-test")).unwrap().state,
        SessionState::Running
    );
}

#[test]
fn semantic_restart_replaces_process_group_under_same_identity() {
    let _lock = lock_pty_tests();
    let registry = SessionRegistry::default();
    registry
        .start(request("restart-test", &["/bin/sh", "-c", "sleep 30"]))
        .unwrap();
    let old_group = registry
        .state
        .lock()
        .unwrap()
        .sessions
        .get("restart-test")
        .unwrap()
        .process_group;

    let mut restart = Request::new(
        1,
        "session.restart",
        serde_json::json!({ "session_id": "restart-test" }),
    );
    restart.operation_id = Some("restart-1".into());
    let response = dispatch(restart, &registry);
    assert!(response.error.is_none(), "{response:?}");
    let outcome: SemanticOutcome = serde_json::from_value(response.result.unwrap()).unwrap();
    assert_eq!(outcome.session_id, "restart-test");
    assert_eq!(outcome.operation_id, "restart-1");
    assert_eq!(response.operation_id.as_deref(), Some("restart-1"));
    let session = outcome.session.expect("restart returns session info");
    assert_eq!(session.session_id, "restart-test");
    assert_eq!(session.state, SessionState::Running);
    assert!(!process_group_exists(old_group));

    let new_group = registry
        .state
        .lock()
        .unwrap()
        .sessions
        .get("restart-test")
        .unwrap()
        .process_group;
    assert_ne!(old_group, new_group);
    assert!(process_group_exists(new_group));

    let events = registry.events.batch("restart-test", 0);
    assert!(events.events.iter().any(|event| {
        matches!(
            event.kind,
            SessionEventKind::Lifecycle {
                state: SessionState::Stopped,
                ..
            }
        )
    }));
    assert!(events.events.iter().any(|event| {
        matches!(
            event.kind,
            SessionEventKind::Lifecycle {
                state: SessionState::Running,
                ..
            }
        )
    }));
    assert!(events.events.iter().any(|event| {
        matches!(
            &event.kind,
            SessionEventKind::Operation {
                operation_id,
                method,
                success: true,
                ..
            } if operation_id == "restart-1" && method == "session.restart"
        )
    }));
}

#[test]
fn semantic_submit_requires_operation_id_and_codex_driver() {
    let _lock = lock_pty_tests();
    let registry = SessionRegistry::default();
    registry
        .start(request("submit-generic", &["/bin/sh", "-c", "sleep 30"]))
        .unwrap();

    let missing = Request::new(
        1,
        "session.submit",
        serde_json::json!({
            "session_id": "submit-generic",
            "message": "hello"
        }),
    );
    let missing_response = dispatch(missing, &registry);
    assert_eq!(
        missing_response.error.as_ref().map(|error| error.code),
        Some(error_codes::INVALID_PARAMS)
    );

    let mut generic = Request::new(
        2,
        "session.submit",
        serde_json::json!({
            "session_id": "submit-generic",
            "message": "hello"
        }),
    );
    generic.operation_id = Some("submit-1".into());
    let generic_response = dispatch(generic, &registry);
    assert_eq!(
        generic_response.error.as_ref().map(|error| error.code),
        Some(error_codes::INVALID_STATE)
    );
    assert!(
        generic_response
            .error
            .unwrap()
            .message
            .contains("only implemented for codex")
    );
}

#[test]
fn codex_submit_waits_for_receipt_before_enter() {
    let _lock = lock_pty_tests();
    let registry = SessionRegistry::default();
    let script = r#"
import sys, time
sys.stdout.write("› "); sys.stdout.flush()
buf = ""
while True:
    ch = sys.stdin.read(1)
    if not ch:
        break
    if ch in ("\n", "\r"):
        break
    buf += ch
    sys.stdout.write(ch)
    sys.stdout.flush()
sys.stdout.write("\nThinking…\n")
sys.stdout.flush()
time.sleep(30)
"#;
    registry
        .start(StartSession {
            session_id: "codex-submit".into(),
            driver: "codex".into(),
            command: vec!["python3".into(), "-u".into(), "-c".into(), script.into()],
            cwd: "/tmp".into(),
            columns: 80,
            rows: 24,
        })
        .unwrap();
    wait_for_screen(&registry, "codex-submit", "›");

    let mut submit = Request::new(
        1,
        "session.submit",
        serde_json::json!({
            "session_id": "codex-submit",
            "message": "hello",
            "timeout_ms": 3_000
        }),
    );
    submit.operation_id = Some("turn-1".into());
    let response = dispatch(submit, &registry);
    assert!(response.error.is_none(), "{response:?}");
    let outcome: SemanticOutcome = serde_json::from_value(response.result.unwrap()).unwrap();
    assert_eq!(outcome.operation_id, "turn-1");
    assert_eq!(outcome.phase.as_deref(), Some("running"));
    assert_eq!(response.operation_id.as_deref(), Some("turn-1"));
    let snapshot = registry.snapshot(selector("codex-submit")).unwrap();
    assert!(snapshot.contents.contains("hello"));
    assert!(snapshot.contents.contains("Thinking"));
}

#[test]
fn codex_submit_returns_uncertain_when_receipt_times_out() {
    let _lock = lock_pty_tests();
    let registry = SessionRegistry::default();
    let script = r#"
import sys, time
sys.stdout.write("› "); sys.stdout.flush()
time.sleep(30)
"#;
    registry
        .start(StartSession {
            session_id: "codex-uncertain".into(),
            driver: "codex".into(),
            command: vec!["python3".into(), "-u".into(), "-c".into(), script.into()],
            cwd: "/tmp".into(),
            columns: 80,
            rows: 24,
        })
        .unwrap();
    wait_for_screen(&registry, "codex-uncertain", "›");

    let mut submit = Request::new(
        1,
        "session.submit",
        serde_json::json!({
            "session_id": "codex-uncertain",
            "message": "hello",
            "timeout_ms": 200
        }),
    );
    submit.operation_id = Some("turn-timeout".into());
    let response = dispatch(submit, &registry);
    assert!(response.error.is_none(), "{response:?}");
    let outcome: SemanticOutcome = serde_json::from_value(response.result.unwrap()).unwrap();
    assert_eq!(outcome.phase.as_deref(), Some("uncertain"));
    // Busy lock must be released after uncertain completion.
    let mut interrupt = Request::new(
        2,
        "session.interrupt",
        serde_json::json!({ "session_id": "codex-uncertain" }),
    );
    interrupt.operation_id = Some("interrupt-after".into());
    let interrupt_response = dispatch(interrupt, &registry);
    assert!(interrupt_response.error.is_none(), "{interrupt_response:?}");
}

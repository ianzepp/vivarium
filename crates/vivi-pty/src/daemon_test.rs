use super::*;
use std::os::unix::net::UnixListener;
use std::sync::Arc;
use std::time::Duration;

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
fn multiple_sessions_can_be_started_and_inspected_concurrently() {
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
fn diagnostic_snapshot_contains_protocol_process_and_terminal_evidence() {
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
    assert_eq!(snapshot.session.session_id, "diagnostic-test");
    assert_eq!(snapshot.terminal.session_id, "diagnostic-test");
}

use std::io::Write;
use std::process::{Command, Output, Stdio};

use serde_json::Value;
use vivarium::storage::Storage;

#[test]
fn local_mail_send_creates_readable_inbox_and_sent_copy() {
    let project = tempfile::tempdir().unwrap();
    init_roster(project.path());

    let output = vivi([
        "mail",
        "send",
        "--project",
        project.path().to_str().unwrap(),
        "--from",
        "ceo",
        "--to",
        "cto",
        "--subject",
        "review: local delivery",
        "--body",
        "Please review the API shape.",
    ]);
    assert_success(&output);
    let send_stdout = stdout(&output);
    let delivered = handle_after(&send_stdout, "delivered cto");
    let sent = handle_after(&send_stdout, "sent");

    let list = vivi([
        "mail",
        "list",
        "--project",
        project.path().to_str().unwrap(),
        "--for",
        "cto",
    ]);
    assert_success(&list);
    let list_stdout = stdout(&list);
    assert!(list_stdout.contains(&delivered), "{list_stdout}");
    assert!(list_stdout.contains("ceo@"), "{list_stdout}");
    assert!(
        list_stdout.contains("review: local delivery"),
        "{list_stdout}"
    );
    // human list includes message date between handle and from
    assert!(
        list_stdout.contains('T') && list_stdout.contains('+') || list_stdout.contains('Z'),
        "expected timestamp in mail list: {list_stdout}"
    );

    let list_json = vivi([
        "mail",
        "list",
        "--project",
        project.path().to_str().unwrap(),
        "--for",
        "cto",
        "--json",
    ]);
    assert_success(&list_json);
    let list_value: Value = serde_json::from_str(&stdout(&list_json)).unwrap();
    let items = list_value.as_array().expect("mail list json array");
    assert_eq!(items.len(), 1, "{list_value}");
    assert_eq!(items[0]["handle"], delivered);
    assert_eq!(items[0]["subject"], "review: local delivery");
    assert!(items[0]["from"].as_str().unwrap().contains("ceo@"));
    assert!(
        !items[0]["date"].as_str().unwrap_or("").is_empty(),
        "{list_value}"
    );
    assert_eq!(items[0]["role"], "inbox");

    let show = vivi([
        "mail",
        "show",
        "--project",
        project.path().to_str().unwrap(),
        &delivered,
    ]);
    assert_success(&show);
    let show_stdout = stdout(&show);
    assert!(
        show_stdout.contains("review: local delivery"),
        "{show_stdout}"
    );
    assert!(
        show_stdout.contains("Please review the API shape."),
        "{show_stdout}"
    );

    let show_json = vivi([
        "mail",
        "show",
        "--project",
        project.path().to_str().unwrap(),
        "--json",
        &delivered,
    ]);
    assert_success(&show_json);
    let show_json_stdout = stdout(&show_json);
    assert!(show_json_stdout.contains(&delivered), "{show_json_stdout}");
    assert!(
        show_json_stdout.contains("review: local delivery"),
        "{show_json_stdout}"
    );

    let sent_list = vivi([
        "mail",
        "list",
        "--project",
        project.path().to_str().unwrap(),
        "--for",
        "ceo",
        "--folder",
        "sent",
    ]);
    assert_success(&sent_list);
    let sent_stdout = stdout(&sent_list);
    assert!(sent_stdout.contains(&sent), "{sent_stdout}");
    assert!(
        sent_stdout.contains("review: local delivery"),
        "{sent_stdout}"
    );

    let storage = Storage::open_mailspace(&project.path().join(".vivi")).unwrap();
    let inbox_raw = String::from_utf8(storage.read_message(&delivered).unwrap()).unwrap();
    assert!(inbox_raw.contains("Subject: review: local delivery"));
    assert!(inbox_raw.contains("Please review the API shape."));

    let sent_raw = String::from_utf8(storage.read_message(&sent).unwrap()).unwrap();
    assert!(sent_raw.contains("Subject: review: local delivery"));
    assert!(sent_raw.contains("Please review the API shape."));
}

#[test]
fn task_send_show_done_and_reopen_reads_expected_task() {
    let project = tempfile::tempdir().unwrap();
    init_roster(project.path());
    let task_file = project.path().join("task.md");
    std::fs::write(&task_file, "Implement and test local delivery.").unwrap();

    let output = vivi([
        "task",
        "send",
        "--project",
        project.path().to_str().unwrap(),
        "--from",
        "ceo",
        "--to",
        "cto",
        "--subject",
        "Implement local delivery",
        "--body",
        &format!("@{}", task_file.display()),
    ]);
    assert_success(&output);
    let send_stdout = stdout(&output);
    let handle = handle_after(&send_stdout, "created cto");

    let open_list = vivi([
        "task",
        "list",
        "--project",
        project.path().to_str().unwrap(),
        "--for",
        "cto",
    ]);
    assert_success(&open_list);
    let open_stdout = stdout(&open_list);
    assert!(open_stdout.contains(&handle), "{open_stdout}");
    assert!(
        open_stdout.contains("Implement local delivery"),
        "{open_stdout}"
    );

    let open_json = vivi([
        "task",
        "list",
        "--project",
        project.path().to_str().unwrap(),
        "--for",
        "cto",
        "--json",
    ]);
    assert_success(&open_json);
    let open_items: Value = serde_json::from_str(&stdout(&open_json)).unwrap();
    let open_item = &open_items.as_array().unwrap()[0];
    assert_eq!(open_item["handle"], handle);
    assert_eq!(open_item["kind"], "task");
    assert_eq!(open_item["status"], "open");
    assert_eq!(open_item["last_event"]["command"], "task send");

    let show = vivi([
        "task",
        "show",
        "--project",
        project.path().to_str().unwrap(),
        &handle,
    ]);
    assert_success(&show);
    let show_stdout = stdout(&show);
    assert!(
        show_stdout.contains("Implement local delivery"),
        "{show_stdout}"
    );
    assert!(
        show_stdout.contains("Implement and test local delivery."),
        "{show_stdout}"
    );

    let done = vivi([
        "task",
        "done",
        "--project",
        project.path().to_str().unwrap(),
        &handle,
        "--for",
        "cto",
    ]);
    assert_success(&done);
    assert!(stdout(&done).contains(&format!("done {handle}")));

    let done_list = vivi([
        "task",
        "list",
        "--project",
        project.path().to_str().unwrap(),
        "--for",
        "cto",
        "--status",
        "done",
    ]);
    assert_success(&done_list);
    let done_stdout = stdout(&done_list);
    assert!(done_stdout.contains(&handle), "{done_stdout}");
    assert!(
        done_stdout.contains("Implement local delivery"),
        "{done_stdout}"
    );

    let done_json = vivi([
        "task",
        "list",
        "--project",
        project.path().to_str().unwrap(),
        "--for",
        "cto",
        "--status",
        "done",
        "--json",
    ]);
    assert_success(&done_json);
    let done_items: Value = serde_json::from_str(&stdout(&done_json)).unwrap();
    let done_item = &done_items.as_array().unwrap()[0];
    assert_eq!(done_item["handle"], handle);
    assert_eq!(done_item["status"], "done");
    assert_eq!(done_item["last_event"]["command"], "task done");

    let reopened = vivi([
        "task",
        "reopen",
        "--project",
        project.path().to_str().unwrap(),
        &handle,
        "--for",
        "cto",
    ]);
    assert_success(&reopened);
    assert!(stdout(&reopened).contains(&format!("reopened {handle}")));
}

#[test]
fn dump_commands_filter_mail_and_tasks_for_board_review() {
    let project = tempfile::tempdir().unwrap();
    init_roster(project.path());

    assert_success(&vivi([
        "mail",
        "send",
        "--project",
        project.path().to_str().unwrap(),
        "--from",
        "ceo",
        "--to",
        "cto",
        "--subject",
        "status: blocker review",
        "--body",
        "The release blocker is now assigned.",
    ]));
    let task = vivi([
        "task",
        "send",
        "--project",
        project.path().to_str().unwrap(),
        "--from",
        "ceo",
        "--to",
        "cto",
        "--subject",
        "Resolve release blocker",
        "--body",
        "Validate the release blocker fix.",
    ]);
    assert_success(&task);
    let task_handle = handle_after(&stdout(&task), "created cto");
    assert_success(&vivi([
        "task",
        "done",
        "--project",
        project.path().to_str().unwrap(),
        &task_handle,
        "--for",
        "cto",
        "--note",
        "Validated blocker fix.",
    ]));

    let mail_dump = vivi([
        "mail",
        "dump",
        "--project",
        project.path().to_str().unwrap(),
        "--participant",
        "cto",
        "--body",
        "release blocker",
    ]);
    assert_success(&mail_dump);
    let mail_stdout = stdout(&mail_dump);
    assert!(mail_stdout.contains("# Vivi Mail Dump"), "{mail_stdout}");
    assert!(
        mail_stdout.contains("status: blocker review"),
        "{mail_stdout}"
    );
    assert!(
        mail_stdout.contains("The release blocker is now assigned."),
        "{mail_stdout}"
    );
    assert!(mail_stdout.contains("Events:"), "{mail_stdout}");
    assert!(mail_stdout.contains("mail send delivered"), "{mail_stdout}");

    let default_task_dump = vivi([
        "task",
        "dump",
        "--project",
        project.path().to_str().unwrap(),
        "--participant",
        "cto",
        "--json",
    ]);
    assert_success(&default_task_dump);
    let default_task_stdout = stdout(&default_task_dump);
    assert!(
        !default_task_stdout.contains(&task_handle),
        "{default_task_stdout}"
    );

    let task_dump = vivi([
        "task",
        "dump",
        "--project",
        project.path().to_str().unwrap(),
        "--participant",
        "cto",
        "--status",
        "all",
        "--json",
    ]);
    assert_success(&task_dump);
    let task_stdout = stdout(&task_dump);
    assert!(task_stdout.contains(&task_handle), "{task_stdout}");
    assert!(
        task_stdout.contains("\"status\": \"done\""),
        "{task_stdout}"
    );
    assert!(task_stdout.contains("\"events\""), "{task_stdout}");
    assert!(
        task_stdout.contains("\"command\": \"task send\""),
        "{task_stdout}"
    );
    assert!(
        task_stdout.contains("\"command\": \"task done\""),
        "{task_stdout}"
    );
    assert!(
        task_stdout.contains("\"event_type\": \"moved\""),
        "{task_stdout}"
    );
    assert!(
        task_stdout.contains("\"note\": \"Validated blocker fix.\""),
        "{task_stdout}"
    );
    assert!(
        task_stdout.contains("Resolve release blocker"),
        "{task_stdout}"
    );
}

#[test]
fn human_stdout_dump_refuses_large_work_exports() {
    let project = tempfile::tempdir().unwrap();
    init_roster(project.path());

    for index in 0..26 {
        assert_success(&vivi([
            "task",
            "send",
            "--project",
            project.path().to_str().unwrap(),
            "--from",
            "ceo",
            "--to",
            "cto",
            "--subject",
            &format!("Bulk task {index}"),
            "--body",
            "Short body.",
        ]));
    }

    let dump = vivi([
        "task",
        "dump",
        "--project",
        project.path().to_str().unwrap(),
        "--for",
        "cto",
    ]);
    assert!(!dump.status.success(), "{}", stdout(&dump));
    assert!(
        stderr(&dump).contains("refusing large human stdout dump"),
        "{}",
        stderr(&dump)
    );

    let output_path = project.path().join("tasks.md");
    let output_dump = vivi([
        "task",
        "dump",
        "--project",
        project.path().to_str().unwrap(),
        "--for",
        "cto",
        "--output",
        output_path.to_str().unwrap(),
    ]);
    assert_success(&output_dump);
    let rendered = std::fs::read_to_string(output_path).unwrap();
    assert!(rendered.contains("count: 26"), "{rendered}");
}

#[test]
fn local_send_reads_body_file_and_stdin() {
    let project = tempfile::tempdir().unwrap();
    init_roster(project.path());
    let body_file = project.path().join("body.md");
    std::fs::write(&body_file, "Body from file.").unwrap();

    let need = vivi([
        "need",
        "send",
        "--project",
        project.path().to_str().unwrap(),
        "--from",
        "ceo",
        "--to",
        "cto",
        "--subject",
        "Read file body",
        "--body-file",
        body_file.to_str().unwrap(),
    ]);
    assert_success(&need);
    let need_handle = handle_after(&stdout(&need), "created cto");

    let shown_need = vivi([
        "need",
        "show",
        "--project",
        project.path().to_str().unwrap(),
        &need_handle,
    ]);
    assert_success(&shown_need);
    assert!(stdout(&shown_need).contains("Body from file."));

    let task = vivi_with_stdin(
        [
            "task",
            "send",
            "--project",
            project.path().to_str().unwrap(),
            "--from",
            "ceo",
            "--to",
            "cto",
            "--subject",
            "Read stdin body",
            "--body",
            "-",
        ],
        "Body from stdin.",
    );
    assert_success(&task);
    let task_handle = handle_after(&stdout(&task), "created cto");

    let shown_task = vivi([
        "task",
        "show",
        "--project",
        project.path().to_str().unwrap(),
        &task_handle,
    ]);
    assert_success(&shown_task);
    assert!(stdout(&shown_task).contains("Body from stdin."));
}

#[test]
fn board_and_status_report_actionable_work() {
    let project = tempfile::tempdir().unwrap();
    init_roster(project.path());

    let task = send_work(project.path(), "task", "cto", "Fix parser", "Patch parser.");
    let need = send_work(
        project.path(),
        "need",
        "cto",
        "Review patch",
        "Review parser.",
    );
    let want_a = send_work(project.path(), "want", "cto", "Track idea A", "Idea A.");
    let want_b = send_work(project.path(), "want", "cto", "Track idea B", "Idea B.");

    let board = vivi([
        "board",
        "--project",
        project.path().to_str().unwrap(),
        "--for",
        "cto",
        "--wants",
        "1",
    ]);
    assert_success(&board);
    let board_stdout = stdout(&board);
    assert!(
        board_stdout.contains("actionable open: 2"),
        "{board_stdout}"
    );
    assert!(board_stdout.contains(&task), "{board_stdout}");
    assert!(board_stdout.contains(&need), "{board_stdout}");
    let visible_wants =
        usize::from(board_stdout.contains(&want_a)) + usize::from(board_stdout.contains(&want_b));
    assert_eq!(visible_wants, 1, "{board_stdout}");
    assert!(
        board_stdout.contains("wants hidden by cap: 1"),
        "{board_stdout}"
    );

    let board_json = vivi([
        "board",
        "--project",
        project.path().to_str().unwrap(),
        "--for",
        "cto",
        "--wants",
        "1",
        "--json",
    ]);
    assert_success(&board_json);
    let board_value: Value = serde_json::from_str(&stdout(&board_json)).unwrap();
    assert_eq!(board_value["totals"]["actionable_open"], 2);
    assert_eq!(board_value["totals"]["wants_open"], 2);
    let identity = &board_value["identities"].as_array().unwrap()[0];
    assert_eq!(identity["actionable_open"], 2);
    assert_eq!(identity["tasks"].as_array().unwrap().len(), 1);
    assert_eq!(identity["needs"].as_array().unwrap().len(), 1);
    assert_eq!(identity["wants"].as_array().unwrap().len(), 1);
    assert_eq!(identity["wants_hidden"], 1);

    let status = vivi([
        "mailspace",
        "status",
        "--project",
        project.path().to_str().unwrap(),
    ]);
    assert_success(&status);
    assert!(
        stdout(&status).contains("total actionable open: 2"),
        "{}",
        stdout(&status)
    );

    let status_json = vivi([
        "mailspace",
        "status",
        "--project",
        project.path().to_str().unwrap(),
        "--json",
    ]);
    assert_success(&status_json);
    let status_value: Value = serde_json::from_str(&stdout(&status_json)).unwrap();
    assert_eq!(status_value["totals"]["actionable_open"], 2);
    assert_eq!(status_value["totals"]["wants_open"], 2);

    let future_board = vivi([
        "board",
        "--project",
        project.path().to_str().unwrap(),
        "--for",
        "cto",
        "--since",
        "2999-01-01T00:00:00Z",
        "--json",
    ]);
    assert_success(&future_board);
    let future_value: Value = serde_json::from_str(&stdout(&future_board)).unwrap();
    assert_eq!(future_value["totals"]["actionable_open"], 0);
    assert_eq!(future_value["identities"][0]["wants_hidden"], 0);

    let watermark = project.path().join("board.watermark");
    std::fs::write(&watermark, "2999-01-01T00:00:00Z").unwrap();
    let watermark_board = vivi([
        "board",
        "--project",
        project.path().to_str().unwrap(),
        "--for",
        "cto",
        "--watermark-file",
        watermark.to_str().unwrap(),
        "--json",
    ]);
    assert_success(&watermark_board);
    let watermark_value: Value = serde_json::from_str(&stdout(&watermark_board)).unwrap();
    assert_eq!(watermark_value["totals"]["actionable_open"], 0);

    let write_watermark = vivi([
        "board",
        "--project",
        project.path().to_str().unwrap(),
        "--for",
        "cto",
        "--since",
        "1h",
        "--watermark-file",
        watermark.to_str().unwrap(),
        "--write-watermark",
        "--json",
    ]);
    assert_success(&write_watermark);
    let written = std::fs::read_to_string(&watermark).unwrap();
    assert!(written.contains('T'), "{written}");
}

#[test]
fn want_done_drop_and_status_lists_closed_wants() {
    let project = tempfile::tempdir().unwrap();
    init_roster(project.path());

    let done_want = send_work(project.path(), "want", "ceo", "Close me", "Obsolete.");
    let dropped_want = send_work(project.path(), "want", "ceo", "Drop me", "Also obsolete.");

    let done = vivi([
        "want",
        "done",
        "--project",
        project.path().to_str().unwrap(),
        &done_want,
        "--for",
        "ceo",
        "--note",
        "No longer needed.",
    ]);
    assert_success(&done);
    assert!(stdout(&done).contains(&format!("done {done_want}")));

    let dropped = vivi([
        "want",
        "drop",
        "--project",
        project.path().to_str().unwrap(),
        &dropped_want,
        "--for",
        "ceo",
    ]);
    assert_success(&dropped);
    assert!(stdout(&dropped).contains(&format!("dropped {dropped_want}")));

    let open = vivi([
        "want",
        "list",
        "--project",
        project.path().to_str().unwrap(),
        "--for",
        "ceo",
    ]);
    assert_success(&open);
    assert!(!stdout(&open).contains(&done_want));
    assert!(!stdout(&open).contains(&dropped_want));

    let done_list = vivi([
        "want",
        "list",
        "--project",
        project.path().to_str().unwrap(),
        "--for",
        "ceo",
        "--status",
        "done",
        "--json",
    ]);
    assert_success(&done_list);
    let done_items: Value = serde_json::from_str(&stdout(&done_list)).unwrap();
    assert_eq!(done_items.as_array().unwrap().len(), 2);
    assert!(stdout(&done_list).contains(&done_want));
    assert!(stdout(&done_list).contains(&dropped_want));
    assert!(stdout(&done_list).contains("\"kind\": \"want\""));
    assert!(stdout(&done_list).contains("\"status\": \"done\""));

    let all_list = vivi([
        "want",
        "list",
        "--project",
        project.path().to_str().unwrap(),
        "--for",
        "ceo",
        "--status",
        "all",
        "--json",
    ]);
    assert_success(&all_list);
    assert!(stdout(&all_list).contains(&done_want));
    assert!(stdout(&all_list).contains(&dropped_want));
}

#[test]
fn want_promotes_to_need_and_done_without_polluting_task_done() {
    let project = tempfile::tempdir().unwrap();
    init_roster(project.path());

    let want = vivi([
        "want",
        "send",
        "--project",
        project.path().to_str().unwrap(),
        "--from",
        "ceo",
        "--to",
        "ceo",
        "--subject",
        "Improve board visibility",
        "--body",
        "Consider a future governance dashboard.",
    ]);
    assert_success(&want);
    let want_handle = handle_after(&stdout(&want), "created ceo");

    let want_list = vivi([
        "want",
        "list",
        "--project",
        project.path().to_str().unwrap(),
        "--for",
        "ceo",
    ]);
    assert_success(&want_list);
    assert!(stdout(&want_list).contains(&want_handle));

    let want_list_json = vivi([
        "want",
        "list",
        "--project",
        project.path().to_str().unwrap(),
        "--for",
        "ceo",
        "--json",
    ]);
    assert_success(&want_list_json);
    let want_items: Value = serde_json::from_str(&stdout(&want_list_json)).unwrap();
    let want_item = &want_items.as_array().unwrap()[0];
    assert_eq!(want_item["handle"], want_handle);
    assert_eq!(want_item["kind"], "want");
    assert_eq!(want_item["status"], "open");

    let promoted = vivi([
        "want",
        "promote",
        "--project",
        project.path().to_str().unwrap(),
        &want_handle,
        "--for",
        "ceo",
        "--note",
        "Prioritize next cycle.",
    ]);
    assert_success(&promoted);
    assert!(stdout(&promoted).contains(&format!("promoted {want_handle}")));

    let need_list = vivi([
        "need",
        "list",
        "--project",
        project.path().to_str().unwrap(),
        "--for",
        "ceo",
    ]);
    assert_success(&need_list);
    assert!(stdout(&need_list).contains(&want_handle));

    let done = vivi([
        "need",
        "done",
        "--project",
        project.path().to_str().unwrap(),
        &want_handle,
        "--for",
        "ceo",
        "--note",
        "Delegated and completed.",
    ]);
    assert_success(&done);
    assert!(stdout(&done).contains(&format!("done {want_handle}")));

    let done_tasks = vivi([
        "task",
        "list",
        "--project",
        project.path().to_str().unwrap(),
        "--for",
        "ceo",
        "--status",
        "done",
    ]);
    assert_success(&done_tasks);
    assert!(!stdout(&done_tasks).contains(&want_handle));

    let default_need_dump = vivi([
        "need",
        "dump",
        "--project",
        project.path().to_str().unwrap(),
        "--for",
        "ceo",
        "--json",
    ]);
    assert_success(&default_need_dump);
    let default_need_stdout = stdout(&default_need_dump);
    assert!(
        !default_need_stdout.contains(&want_handle),
        "{default_need_stdout}"
    );

    let done_needs = vivi([
        "need",
        "dump",
        "--project",
        project.path().to_str().unwrap(),
        "--for",
        "ceo",
        "--status",
        "done",
        "--json",
    ]);
    assert_success(&done_needs);
    let done_needs_stdout = stdout(&done_needs);
    assert!(
        done_needs_stdout.contains(&want_handle),
        "{done_needs_stdout}"
    );
    assert!(
        done_needs_stdout.contains("\"command\": \"want promote\""),
        "{done_needs_stdout}"
    );
    assert!(
        done_needs_stdout.contains("\"command\": \"need done\""),
        "{done_needs_stdout}"
    );
    assert!(
        done_needs_stdout.contains("\"kind\": \"need\""),
        "{done_needs_stdout}"
    );
}

#[test]
fn replies_and_lifecycle_notes_assemble_cross_kind_threads() {
    let project = tempfile::tempdir().unwrap();
    init_roster(project.path());

    let need = send_work(
        project.path(),
        "need",
        "cto",
        "Review API",
        "Please review.",
    );
    let reply = vivi([
        "mail",
        "reply",
        &need,
        "--project",
        project.path().to_str().unwrap(),
        "--from",
        "cto",
        "--body",
        "I reviewed it.",
    ]);
    assert_success(&reply);
    let reply_handle = handle_after(&stdout(&reply), "replied ceo");

    let task = vivi([
        "task",
        "send",
        "--project",
        project.path().to_str().unwrap(),
        "--from",
        "cto",
        "--to",
        "ceo",
        "--subject",
        "Implement follow-up",
        "--body",
        "Follow up on the review.",
        "--reply-to",
        &reply_handle,
    ]);
    assert_success(&task);
    let task_handle = handle_after(&stdout(&task), "created ceo");

    let thread = vivi([
        "mail",
        "thread",
        "--project",
        project.path().to_str().unwrap(),
        "--json",
        &task_handle,
    ]);
    assert_success(&thread);
    let messages: Value = serde_json::from_str(&stdout(&thread)).unwrap();
    assert_eq!(messages.as_array().unwrap().len(), 3);
    assert!(
        messages
            .as_array()
            .unwrap()
            .iter()
            .any(|message| message["kind"] == "task")
    );
    assert!(
        messages
            .as_array()
            .unwrap()
            .iter()
            .skip(1)
            .any(|message| message["link_source"] == "captured")
    );

    let done = vivi([
        "task",
        "done",
        "--project",
        project.path().to_str().unwrap(),
        &task_handle,
        "--for",
        "ceo",
        "--note",
        "Follow-up is complete.",
    ]);
    assert_success(&done);

    let done_thread = vivi([
        "task",
        "show",
        "--project",
        project.path().to_str().unwrap(),
        "--json",
        &task_handle,
    ]);
    assert_success(&done_thread);
    let done_messages: Value = serde_json::from_str(&stdout(&done_thread)).unwrap();
    assert_eq!(done_messages.as_array().unwrap().len(), 4);
    assert!(stdout(&done_thread).contains("Follow-up is complete."));

    let dump = vivi([
        "mail",
        "dump",
        "--project",
        project.path().to_str().unwrap(),
        "--for",
        "ceo",
        "--folder",
        "inbox",
        "--json",
    ]);
    assert_success(&dump);
    assert!(stdout(&dump).contains("parent_content_id"));
    assert!(stdout(&dump).contains("captured"));
}

#[test]
fn mailspace_watch_filters_events_and_advances_cursor() {
    let project = tempfile::tempdir().unwrap();
    init_roster(project.path());
    let task = send_work(
        project.path(),
        "task",
        "cto",
        "Turn end: test",
        "Work finished.",
    );
    let cursor = project.path().join("watch.cursor");

    let first = vivi([
        "mailspace",
        "watch",
        "--project",
        project.path().to_str().unwrap(),
        "--for",
        "cto",
        "--kinds",
        "task",
        "--events",
        "delivered",
        "--match-subject-prefix",
        "Turn end:",
        "--cursor-file",
        cursor.to_str().unwrap(),
        "--write-cursor",
        "--once",
        "--json",
    ]);
    assert_success(&first);
    let event: Value = serde_json::from_str(stdout(&first).trim()).unwrap();
    assert_eq!(event["event"], "delivered");
    assert_eq!(event["kind"], "task");
    assert_eq!(event["for"], "cto");
    assert_eq!(event["handle"], task);
    let cursor_value = std::fs::read_to_string(&cursor).unwrap();
    assert!(!cursor_value.trim().is_empty());

    let done = vivi([
        "task",
        "done",
        "--project",
        project.path().to_str().unwrap(),
        &task,
        "--for",
        "cto",
    ]);
    assert_success(&done);

    let moved = vivi([
        "task",
        "watch",
        "--project",
        project.path().to_str().unwrap(),
        "--for",
        "cto",
        "--handle",
        &task,
        "--events",
        "moved",
        "--statuses",
        "done",
        "--timeout",
        "2s",
        "--json",
    ]);
    assert_success(&moved);
    let moved_event: Value = serde_json::from_str(stdout(&moved).trim()).unwrap();
    assert_eq!(moved_event["status"], "done");
    assert_eq!(moved_event["handle"], task);
}

#[test]
fn inferred_thread_links_are_opt_in_and_marked() {
    let project = tempfile::tempdir().unwrap();
    init_roster(project.path());
    let first = vivi([
        "mail",
        "send",
        "--project",
        project.path().to_str().unwrap(),
        "--from",
        "ceo",
        "--to",
        "cto",
        "--subject",
        "Status report",
        "--body",
        "Initial status.",
    ]);
    assert_success(&first);
    let first_handle = handle_after(&stdout(&first), "delivered cto");
    let second = vivi([
        "mail",
        "send",
        "--project",
        project.path().to_str().unwrap(),
        "--from",
        "cto",
        "--to",
        "ceo",
        "--subject",
        "Re: Status report",
        "--body",
        &format!("Answering {first_handle}: current status."),
    ]);
    assert_success(&second);
    let second_handle = handle_after(&stdout(&second), "delivered ceo");

    let without_inference = vivi([
        "mail",
        "thread",
        "--project",
        project.path().to_str().unwrap(),
        "--json",
        &second_handle,
    ]);
    assert_success(&without_inference);
    assert_eq!(
        serde_json::from_str::<Value>(&stdout(&without_inference))
            .unwrap()
            .as_array()
            .unwrap()
            .len(),
        1
    );

    let inferred = vivi([
        "mail",
        "thread",
        "--project",
        project.path().to_str().unwrap(),
        "--infer",
        "--json",
        &second_handle,
    ]);
    assert_success(&inferred);
    let messages: Value = serde_json::from_str(&stdout(&inferred)).unwrap();
    assert_eq!(messages.as_array().unwrap().len(), 2);
    assert!(
        messages
            .as_array()
            .unwrap()
            .iter()
            .any(|message| message["inferred"] == true && message["link_source"] == "inferred")
    );
}

fn init_roster(project: &std::path::Path) {
    assert_success(&vivi([
        "mailspace",
        "init",
        "--project",
        project.to_str().unwrap(),
    ]));
    for identity in ["ceo", "cto"] {
        assert_success(&vivi([
            "mailspace",
            "identity",
            "add",
            identity,
            "--project",
            project.to_str().unwrap(),
        ]));
    }
}

fn send_work(project: &std::path::Path, kind: &str, to: &str, subject: &str, body: &str) -> String {
    let output = vivi([
        kind,
        "send",
        "--project",
        project.to_str().unwrap(),
        "--from",
        "ceo",
        "--to",
        to,
        "--subject",
        subject,
        "--body",
        body,
    ]);
    assert_success(&output);
    handle_after(&stdout(&output), &format!("created {to}"))
}

fn vivi<I, S>(args: I) -> Output
where
    I: IntoIterator<Item = S>,
    S: AsRef<std::ffi::OsStr>,
{
    Command::new(env!("CARGO_BIN_EXE_vivi"))
        .args(args)
        .output()
        .unwrap()
}

fn vivi_with_stdin<I, S>(args: I, stdin: &str) -> Output
where
    I: IntoIterator<Item = S>,
    S: AsRef<std::ffi::OsStr>,
{
    let mut child = Command::new(env!("CARGO_BIN_EXE_vivi"))
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .as_mut()
        .unwrap()
        .write_all(stdin.as_bytes())
        .unwrap();
    child.wait_with_output().unwrap()
}

fn assert_success(output: &Output) {
    assert!(
        output.status.success(),
        "status: {}\nstdout:\n{}\nstderr:\n{}",
        output.status,
        stdout(output),
        stderr(output)
    );
}

fn stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

fn handle_after(output: &str, prefix: &str) -> String {
    output
        .lines()
        .find_map(|line| line.strip_prefix(prefix))
        .and_then(|rest| rest.split_whitespace().next())
        .unwrap_or_else(|| panic!("missing '{prefix}' in output:\n{output}"))
        .to_string()
}

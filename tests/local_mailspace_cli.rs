use std::process::{Command, Output};

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

use std::process::{Command, Output};

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

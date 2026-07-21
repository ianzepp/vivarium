use std::path::PathBuf;

use clap::Parser;
use vivarium::cli::{
    AgentCommand, Cli, Command, CycleCommand, EnqueueCommand, ExecCommand, IndexCommand,
    MailAbsorbStatus, MailCommand, MailspaceCommand, MailspaceIdentityCommand, MemoCommand,
    NeedCommand, ProtonCommand, QueueCommand, TaskCommand, TaskDumpStatusArg, TaskSendCommand,
    TaskStatus, WantCommand,
};

#[test]
fn parses_render_explain_and_engine() {
    let cli = Cli::try_parse_from([
        "vivi",
        "render",
        "--explain",
        "--format",
        "pdf",
        "--engine",
        "pandoc-tectonic",
    ])
    .unwrap();

    match cli.command {
        Command::Render(command) => {
            assert!(command.explain);
            assert_eq!(command.engine.as_deref(), Some("pandoc-tectonic"));
        }
        other => panic!("unexpected command: {other:?}"),
    }
}

#[test]
fn parses_compose_document_attachments() {
    let cli = Cli::try_parse_from([
        "vivi",
        "compose",
        "--to",
        "a@example.com",
        "--subject",
        "report",
        "--attach",
        "notes.txt",
        "--attach-document",
        "report.md",
    ])
    .unwrap();

    match cli.command {
        Command::Compose(command) => {
            assert_eq!(command.attachments, [std::path::PathBuf::from("notes.txt")]);
            assert_eq!(
                command.attach_document,
                Some(std::path::PathBuf::from("report.md"))
            );
        }
        other => panic!("unexpected command: {other:?}"),
    }
}

#[test]
fn parses_sync_index_and_embed() {
    let cli = Cli::try_parse_from(["vivi", "sync", "--index", "--embed"]).unwrap();

    match cli.command {
        Command::Sync { index, embed, .. } => {
            assert!(index);
            assert!(embed);
        }
        other => panic!("unexpected command: {other:?}"),
    }
}

#[test]
fn parses_sync_embed_without_index() {
    let cli = Cli::try_parse_from(["vivi", "sync", "--embed"]).unwrap();

    match cli.command {
        Command::Sync { index, embed, .. } => {
            assert!(!index);
            assert!(embed);
        }
        other => panic!("unexpected command: {other:?}"),
    }
}

#[test]
fn parses_sync_json() {
    let cli = Cli::try_parse_from(["vivi", "sync", "--json"]).unwrap();

    match cli.command {
        Command::Sync { json, .. } => assert!(json),
        other => panic!("unexpected command: {other:?}"),
    }
}

#[test]
fn parses_watch_inbox_event_contract() {
    let cli = Cli::try_parse_from(["vivi", "watch-inbox", "--account", "agent", "--json"]).unwrap();

    match cli.command {
        Command::WatchInbox { account, json } => {
            assert_eq!(account.as_deref(), Some("agent"));
            assert!(json);
        }
        other => panic!("unexpected command: {other:?}"),
    }
}

#[test]
fn old_outbox_coupled_watch_surface_is_removed() {
    assert!(Cli::try_parse_from(["vivi", "watch"]).is_err());
}

#[test]
fn parses_sync_events_watch() {
    let cli = Cli::try_parse_from([
        "vivi",
        "sync-events",
        "--account",
        "agent",
        "--bootstrap",
        "--watch",
        "--interval",
        "5m",
        "--json",
    ])
    .unwrap();

    match cli.command {
        Command::SyncEvents {
            account,
            bootstrap,
            watch,
            interval,
            json,
        } => {
            assert_eq!(account.as_deref(), Some("agent"));
            assert!(bootstrap);
            assert!(watch);
            assert_eq!(interval, "5m");
            assert!(json);
        }
        other => panic!("unexpected command: {other:?}"),
    }
}

#[test]
fn parses_list_filter() {
    let cli = Cli::try_parse_from([
        "vivi", "list", "inbox", "--filter", "DoorDash", "--unread", "--json",
    ])
    .unwrap();

    match cli.command {
        Command::List {
            folder,
            filter,
            unread,
            read,
            starred,
            unstarred,
            json,
            ..
        } => {
            assert_eq!(folder, "inbox");
            assert_eq!(filter.as_deref(), Some("DoorDash"));
            assert!(unread);
            assert!(!read);
            assert!(!starred);
            assert!(!unstarred);
            assert!(json);
        }
        other => panic!("unexpected command: {other:?}"),
    }
}

#[test]
fn parses_list_starred_filter() {
    let cli = Cli::try_parse_from(["vivi", "list", "--starred"]).unwrap();

    match cli.command {
        Command::List {
            folder,
            starred,
            unstarred,
            ..
        } => {
            assert_eq!(folder, "inbox");
            assert!(starred);
            assert!(!unstarred);
        }
        other => panic!("unexpected command: {other:?}"),
    }
}

#[test]
fn parses_list_flagged_alias() {
    let cli = Cli::try_parse_from(["vivi", "list", "--flagged"]).unwrap();

    match cli.command {
        Command::List { starred, .. } => assert!(starred),
        other => panic!("unexpected command: {other:?}"),
    }
}

#[test]
fn rejects_multiple_list_star_modes() {
    let err = Cli::try_parse_from(["vivi", "list", "--starred", "--unstarred"]).unwrap_err();

    assert_eq!(err.kind(), clap::error::ErrorKind::ArgumentConflict);
}

#[test]
fn parses_mailspace_identity_add() {
    let cli = Cli::try_parse_from(["vivi", "mailspace", "identity", "add", "cto"]).unwrap();

    match cli.command {
        Command::Mailspace {
            command:
                MailspaceCommand::Identity {
                    command: MailspaceIdentityCommand::Add { identity, project },
                },
        } => {
            assert_eq!(identity, "cto");
            assert_eq!(project, None);
        }
        other => panic!("unexpected command: {other:?}"),
    }
}

#[test]
fn parses_role_add_set_and_charter() {
    use vivarium::cli::{RoleCharterCommand, RoleCommand};

    let add = Cli::try_parse_from([
        "vivi",
        "role",
        "add",
        "head-ceo",
        "--kind",
        "head",
        "--harness",
        "subagent",
        "--label",
        "executive",
    ])
    .unwrap();
    match add.command {
        Command::Role {
            command:
                RoleCommand::Add {
                    name,
                    kind,
                    harness,
                    labels,
                    ..
                },
        } => {
            assert_eq!(name, "head-ceo");
            assert_eq!(kind.as_deref(), Some("head"));
            assert_eq!(harness.as_deref(), Some("subagent"));
            assert_eq!(labels, vec!["executive".to_string()]);
        }
        other => panic!("unexpected command: {other:?}"),
    }

    let set = Cli::try_parse_from([
        "vivi",
        "role",
        "set",
        "hand-1",
        "--provider",
        "zai",
        "--model",
        "glm-5.2",
        "--thinking",
        "low",
    ])
    .unwrap();
    match set.command {
        Command::Role {
            command:
                RoleCommand::Set {
                    name,
                    provider,
                    model,
                    thinking,
                    ..
                },
        } => {
            assert_eq!(name, "hand-1");
            assert_eq!(provider.as_deref(), Some("zai"));
            assert_eq!(model.as_deref(), Some("glm-5.2"));
            assert_eq!(thinking.as_deref(), Some("low"));
        }
        other => panic!("unexpected command: {other:?}"),
    }

    let charter = Cli::try_parse_from([
        "vivi",
        "role",
        "charter",
        "set",
        "head-ceo",
        "--file",
        "personas/ceo.md",
    ])
    .unwrap();
    match charter.command {
        Command::Role {
            command:
                RoleCommand::Charter {
                    command: RoleCharterCommand::Set { name, file, .. },
                },
        } => {
            assert_eq!(name, "head-ceo");
            assert_eq!(
                file.as_ref().map(|p| p.to_string_lossy().into_owned()),
                Some("personas/ceo.md".into())
            );
        }
        other => panic!("unexpected command: {other:?}"),
    }
}

#[test]
fn parses_role_set_pid_host_and_status() {
    use vivarium::cli::RoleCommand;

    let set = Cli::try_parse_from([
        "vivi", "role", "set", "hand-1", "--pid", "12345", "--host", "pharos",
    ])
    .unwrap();
    match set.command {
        Command::Role {
            command:
                RoleCommand::Set {
                    name,
                    pid,
                    host,
                    clear_pid,
                    clear_host,
                    ..
                },
        } => {
            assert_eq!(name, "hand-1");
            assert_eq!(pid, Some(12_345));
            assert_eq!(host.as_deref(), Some("pharos"));
            assert!(!clear_pid);
            assert!(!clear_host);
        }
        other => panic!("unexpected command: {other:?}"),
    }

    let clear = Cli::try_parse_from(["vivi", "role", "set", "hand-1", "--clear-pid"]).unwrap();
    match clear.command {
        Command::Role {
            command: RoleCommand::Set {
                name, clear_pid, ..
            },
        } => {
            assert_eq!(name, "hand-1");
            assert!(clear_pid);
        }
        other => panic!("unexpected command: {other:?}"),
    }

    let status = Cli::try_parse_from(["vivi", "role", "status", "hand-1", "--json"]).unwrap();
    match status.command {
        Command::Role {
            command: RoleCommand::Status { name, json, .. },
        } => {
            assert_eq!(name, "hand-1");
            assert!(json);
        }
        other => panic!("unexpected command: {other:?}"),
    }
}

#[test]
fn parses_mailspace_import_dry_run() {
    let cli = Cli::try_parse_from([
        "vivi",
        "mailspace",
        "import",
        "--project",
        "/tmp/target",
        "--from",
        "/tmp/source",
        "--dry-run",
        "--json",
    ])
    .unwrap();

    match cli.command {
        Command::Mailspace {
            command: MailspaceCommand::Import(command),
        } => {
            assert_eq!(
                command.project.unwrap(),
                std::path::PathBuf::from("/tmp/target")
            );
            assert_eq!(command.from, std::path::PathBuf::from("/tmp/source"));
            assert!(command.dry_run);
            assert!(command.json);
        }
        other => panic!("unexpected command: {other:?}"),
    }
}

#[test]
fn parses_mailspace_merge_compatibility_alias() {
    let cli = Cli::try_parse_from([
        "vivi",
        "mailspace",
        "merge",
        "--from",
        "/tmp/source",
        "--dry-run",
    ])
    .unwrap();

    match cli.command {
        Command::Mailspace {
            command: MailspaceCommand::Merge(command),
        } => {
            assert_eq!(command.from, std::path::PathBuf::from("/tmp/source"));
            assert!(command.dry_run);
        }
        other => panic!("unexpected command: {other:?}"),
    }
}

#[test]
fn parses_board_command() {
    let cli = Cli::try_parse_from([
        "vivi",
        "board",
        "--for",
        "cto",
        "--wants",
        "3",
        "--since",
        "1h",
        "--watermark-file",
        "/tmp/board.watermark",
        "--write-watermark",
        "--json",
        "--project",
        "/tmp/project",
    ])
    .unwrap();

    match cli.command {
        Command::Board(command) => {
            assert_eq!(command.for_identity.as_deref(), Some("cto"));
            assert_eq!(command.wants, 3);
            assert_eq!(command.since.as_deref(), Some("1h"));
            assert_eq!(
                command.watermark_file,
                Some(PathBuf::from("/tmp/board.watermark"))
            );
            assert!(command.write_watermark);
            assert!(command.json);
            assert_eq!(command.project, Some(PathBuf::from("/tmp/project")));
        }
        other => panic!("unexpected command: {other:?}"),
    }
}

#[test]
fn parses_board_with_global_project_before_subcommand() {
    let cli = Cli::try_parse_from([
        "vivi",
        "--project",
        "/tmp/project",
        "board",
        "--for",
        "mind",
        "--json",
    ])
    .unwrap();

    assert_eq!(cli.project, Some(PathBuf::from("/tmp/project")));
    match cli.command {
        Command::Board(command) => {
            assert_eq!(command.for_identity.as_deref(), Some("mind"));
            assert!(command.json);
            // clap global --project also fills the board-local project field
            assert_eq!(command.project, Some(PathBuf::from("/tmp/project")));
        }
        other => panic!("unexpected command: {other:?}"),
    }
}

#[test]
fn parses_task_list_with_global_project_before_subcommand() {
    let cli = Cli::try_parse_from([
        "vivi",
        "--project",
        "/tmp/project",
        "task",
        "list",
        "--for",
        "hand-1",
        "--status",
        "open",
    ])
    .unwrap();

    assert_eq!(cli.project, Some(PathBuf::from("/tmp/project")));
    match cli.command {
        Command::Task {
            command:
                TaskCommand::List {
                    for_identity,
                    status,
                    project,
                    ..
                },
        } => {
            assert_eq!(for_identity, "hand-1");
            assert!(matches!(status, TaskStatus::Open));
            assert_eq!(project, Some(PathBuf::from("/tmp/project")));
        }
        other => panic!("unexpected command: {other:?}"),
    }
}

#[test]
fn parses_local_mail_send() {
    let cli = Cli::try_parse_from([
        "vivi",
        "mail",
        "send",
        "--from",
        "ceo",
        "--to",
        "cto",
        "--subject",
        "review",
        "--body",
        "please review",
    ])
    .unwrap();

    match cli.command {
        Command::Mail {
            command: MailCommand::Send(command),
        } => {
            assert_eq!(command.from, "ceo");
            assert_eq!(command.to, vec!["cto"]);
            assert_eq!(command.subject, "review");
            assert_eq!(command.body.as_deref(), Some("please review"));
            assert_eq!(command.body_file, None);
        }
        other => panic!("unexpected command: {other:?}"),
    }
}

#[test]
fn parses_local_mail_send_body_file() {
    let cli = Cli::try_parse_from([
        "vivi",
        "mail",
        "send",
        "--from",
        "ceo",
        "--to",
        "cto",
        "--subject",
        "review",
        "--body-file",
        "body.md",
    ])
    .unwrap();

    match cli.command {
        Command::Mail {
            command: MailCommand::Send(command),
        } => {
            assert_eq!(command.body, None);
            assert_eq!(command.body_file, Some(PathBuf::from("body.md")));
        }
        other => panic!("unexpected command: {other:?}"),
    }
}

#[test]
fn parses_local_mail_list_with_json_and_project() {
    let cli = Cli::try_parse_from([
        "vivi",
        "mail",
        "list",
        "--for",
        "mind",
        "--folder",
        "inbox",
        "--json",
        "--project",
        "/tmp/project",
    ])
    .unwrap();

    match cli.command {
        Command::Mail {
            command:
                MailCommand::List {
                    for_identity,
                    folder,
                    status,
                    absorbed_by,
                    json,
                    project,
                },
        } => {
            assert_eq!(for_identity, "mind");
            assert_eq!(folder, "inbox");
            assert!(matches!(status, MailAbsorbStatus::All));
            assert_eq!(absorbed_by, None);
            assert!(json);
            assert_eq!(project, Some(PathBuf::from("/tmp/project")));
        }
        other => panic!("unexpected command: {other:?}"),
    }
}

#[test]
fn parses_local_mail_show() {
    let cli = Cli::try_parse_from(["vivi", "mail", "show", "abc123", "--json"]).unwrap();

    match cli.command {
        Command::Mail {
            command:
                MailCommand::Show {
                    handles,
                    json,
                    project,
                },
        } => {
            assert_eq!(handles, vec!["abc123"]);
            assert!(json);
            assert_eq!(project, None);
        }
        other => panic!("unexpected command: {other:?}"),
    }
}

#[test]
fn parses_trace_command() {
    let cli = Cli::try_parse_from([
        "vivi",
        "trace",
        "abc123",
        "--json",
        "--max-depth",
        "5",
        "--limit",
        "100",
        "--project",
        "/tmp/project",
    ])
    .unwrap();

    match cli.command {
        Command::Trace(command) => {
            assert_eq!(command.handle, "abc123");
            assert!(command.json);
            assert_eq!(command.max_depth, 5);
            assert_eq!(command.limit, 100);
            assert_eq!(command.project, Some(PathBuf::from("/tmp/project")));
        }
        other => panic!("unexpected command: {other:?}"),
    }
}

#[test]
fn parses_local_mail_dump_filters() {
    let cli = Cli::try_parse_from([
        "vivi",
        "mail",
        "dump",
        "--participant",
        "cto",
        "--from",
        "ceo",
        "--subject",
        "review",
        "--body",
        "blocker",
        "--since",
        "24h",
        "--json",
    ])
    .unwrap();

    match cli.command {
        Command::Mail {
            command: MailCommand::Dump(command),
        } => {
            assert_eq!(command.participant.as_deref(), Some("cto"));
            assert_eq!(command.from.as_deref(), Some("ceo"));
            assert_eq!(command.subject.as_deref(), Some("review"));
            assert_eq!(command.body.as_deref(), Some("blocker"));
            assert_eq!(command.since.as_deref(), Some("24h"));
            assert!(command.json);
        }
        other => panic!("unexpected command: {other:?}"),
    }
}

#[test]
fn parses_task_done() {
    let cli = Cli::try_parse_from(["vivi", "task", "done", "abc123", "--for", "cto"]).unwrap();

    match cli.command {
        Command::Task {
            command:
                TaskCommand::Done {
                    handle,
                    for_identity,
                    note,
                    verdict,
                    repo,
                    tip,
                    project,
                },
        } => {
            assert_eq!(handle, "abc123");
            assert_eq!(for_identity, "cto");
            assert_eq!(note, None);
            assert_eq!(verdict, None);
            assert!(repo.is_empty());
            assert!(tip.is_empty());
            assert_eq!(project, None);
        }
        other => panic!("unexpected command: {other:?}"),
    }
}

#[test]
fn parses_task_done_with_verdict_and_tips() {
    let cli = Cli::try_parse_from([
        "vivi",
        "task",
        "done",
        "abc123",
        "--for",
        "auditor-1",
        "--verdict",
        "clean_pass",
        "--repo",
        "examples",
        "--tip",
        "e968cc3",
        "--repo",
        "hosts",
        "--tip",
        "0de5c36",
        "--note",
        "P2: minor lint",
    ])
    .unwrap();

    match cli.command {
        Command::Task {
            command:
                TaskCommand::Done {
                    handle,
                    for_identity,
                    note,
                    verdict,
                    repo,
                    tip,
                    project,
                },
        } => {
            assert_eq!(handle, "abc123");
            assert_eq!(for_identity, "auditor-1");
            assert_eq!(note.as_deref(), Some("P2: minor lint"));
            assert_eq!(verdict.as_deref(), Some("clean_pass"));
            assert_eq!(repo, vec!["examples", "hosts"]);
            assert_eq!(tip, vec!["e968cc3", "0de5c36"]);
            assert_eq!(project, None);
        }
        other => panic!("unexpected command: {other:?}"),
    }
}

#[test]
fn parses_task_list_done_status() {
    let cli =
        Cli::try_parse_from(["vivi", "task", "list", "--for", "cto", "--status", "done"]).unwrap();

    match cli.command {
        Command::Task {
            command:
                TaskCommand::List {
                    for_identity,
                    status,
                    json,
                    project,
                    blocked: _,
                    blocking: _,
                },
        } => {
            assert_eq!(for_identity, "cto");
            assert!(matches!(status, TaskStatus::Done));
            assert!(!json);
            assert_eq!(project, None);
        }
        other => panic!("unexpected command: {other:?}"),
    }
}

#[test]
fn parses_task_list_json() {
    let cli = Cli::try_parse_from(["vivi", "task", "list", "--for", "cto", "--json"]).unwrap();

    match cli.command {
        Command::Task {
            command: TaskCommand::List { json, .. },
        } => assert!(json),
        other => panic!("unexpected command: {other:?}"),
    }
}

#[test]
fn task_dump_defaults_to_open_status() {
    let cli = Cli::try_parse_from(["vivi", "task", "dump", "--for", "cto"]).unwrap();

    match cli.command {
        Command::Task {
            command: TaskCommand::Dump(command),
        } => assert!(matches!(command.status, TaskDumpStatusArg::Open)),
        other => panic!("unexpected command: {other:?}"),
    }
}

#[test]
fn parses_task_dump_status_all() {
    let cli = Cli::try_parse_from([
        "vivi", "task", "dump", "--for", "cto", "--status", "all", "--output", "tasks.md",
    ])
    .unwrap();

    match cli.command {
        Command::Task {
            command: TaskCommand::Dump(command),
        } => {
            assert_eq!(command.for_identity.as_deref(), Some("cto"));
            assert!(matches!(command.status, TaskDumpStatusArg::All));
            assert_eq!(command.output, Some(PathBuf::from("tasks.md")));
        }
        other => panic!("unexpected command: {other:?}"),
    }
}

#[test]
fn parses_need_done_with_note() {
    let cli = Cli::try_parse_from([
        "vivi",
        "need",
        "done",
        "abc123",
        "--for",
        "ceo",
        "--note",
        "tasks completed",
    ])
    .unwrap();

    match cli.command {
        Command::Need {
            command:
                NeedCommand::Done {
                    handle,
                    for_identity,
                    note,
                    project,
                },
        } => {
            assert_eq!(handle, "abc123");
            assert_eq!(for_identity, "ceo");
            assert_eq!(note.as_deref(), Some("tasks completed"));
            assert_eq!(project, None);
        }
        other => panic!("unexpected command: {other:?}"),
    }
}

#[test]
fn parses_want_promote() {
    let cli = Cli::try_parse_from(["vivi", "want", "promote", "abc123", "--for", "ceo"]).unwrap();

    match cli.command {
        Command::Want {
            command:
                WantCommand::Promote {
                    handle,
                    for_identity,
                    note,
                    project,
                },
        } => {
            assert_eq!(handle, "abc123");
            assert_eq!(for_identity, "ceo");
            assert_eq!(note, None);
            assert_eq!(project, None);
        }
        other => panic!("unexpected command: {other:?}"),
    }
}

#[test]
fn parses_doctor_json() {
    let cli = Cli::try_parse_from(["vivi", "doctor", "--account", "proton", "--json"]).unwrap();

    match cli.command {
        Command::Doctor { account, json } => {
            assert_eq!(account.as_deref(), Some("proton"));
            assert!(json);
        }
        other => panic!("unexpected command: {other:?}"),
    }
}

#[test]
fn parses_proton_auth_info_json() {
    let cli = Cli::try_parse_from([
        "vivi",
        "proton",
        "auth-info",
        "--account",
        "agent",
        "--json",
    ])
    .unwrap();

    match cli.command {
        Command::Proton {
            command: ProtonCommand::AuthInfo { account, json },
        } => {
            assert_eq!(account.as_deref(), Some("agent"));
            assert!(json);
        }
        other => panic!("unexpected command: {other:?}"),
    }
}

#[test]
fn parses_proton_login_check_totp() {
    let cli = Cli::try_parse_from([
        "vivi",
        "proton",
        "login-check",
        "--account",
        "agent",
        "--totp-code",
        "123456",
    ])
    .unwrap();

    match cli.command {
        Command::Proton {
            command:
                ProtonCommand::LoginCheck {
                    account,
                    totp_code,
                    json,
                },
        } => {
            assert_eq!(account.as_deref(), Some("agent"));
            assert_eq!(totp_code.as_deref(), Some("123456"));
            assert!(!json);
        }
        other => panic!("unexpected command: {other:?}"),
    }
}

#[test]
fn parses_proton_login_json() {
    let cli =
        Cli::try_parse_from(["vivi", "proton", "login", "--account", "agent", "--json"]).unwrap();

    match cli.command {
        Command::Proton {
            command:
                ProtonCommand::Login {
                    account,
                    totp_code,
                    json,
                },
        } => {
            assert_eq!(account.as_deref(), Some("agent"));
            assert_eq!(totp_code, None);
            assert!(json);
        }
        other => panic!("unexpected command: {other:?}"),
    }
}

#[test]
fn parses_proton_identity_json() {
    let cli = Cli::try_parse_from(["vivi", "proton", "identity", "--account", "agent", "--json"])
        .unwrap();

    match cli.command {
        Command::Proton {
            command: ProtonCommand::Identity { account, json },
        } => {
            assert_eq!(account.as_deref(), Some("agent"));
            assert!(json);
        }
        other => panic!("unexpected command: {other:?}"),
    }
}

#[test]
fn parses_proton_session_check_json() {
    let cli = Cli::try_parse_from([
        "vivi",
        "proton",
        "session-check",
        "--account",
        "agent",
        "--json",
    ])
    .unwrap();

    match cli.command {
        Command::Proton {
            command: ProtonCommand::SessionCheck { account, json },
        } => {
            assert_eq!(account.as_deref(), Some("agent"));
            assert!(json);
        }
        other => panic!("unexpected command: {other:?}"),
    }
}

#[test]
fn parses_default_compose_command() {
    let cli = Cli::try_parse_from([
        "vivi",
        "compose",
        "--to",
        "a@example.com",
        "--cc",
        "b@example.com",
        "--bcc",
        "c@example.com",
        "--subject",
        "hello",
        "--body",
        "body",
        "--append-remote",
    ])
    .unwrap();

    match cli.command {
        Command::Compose(vivarium::cli::ComposeCommand {
            to,
            cc,
            bcc,
            from,
            subject,
            body,
            html_body,
            html_body_auto,
            append_remote,
            ..
        }) => {
            assert_eq!(to, vec!["a@example.com"]);
            assert_eq!(cc, vec!["b@example.com"]);
            assert_eq!(bcc, vec!["c@example.com"]);
            assert_eq!(from, None);
            assert_eq!(subject, "hello");
            assert_eq!(body.as_deref(), Some("body"));
            assert_eq!(html_body, None);
            assert!(!html_body_auto);
            assert!(append_remote);
        }
        other => panic!("unexpected command: {other:?}"),
    }
}

#[test]
fn parses_compose_html_body_auto() {
    let cli = Cli::try_parse_from([
        "vivi",
        "compose",
        "--to",
        "a@example.com",
        "--subject",
        "hello",
        "--body",
        "body",
        "--html-body-auto",
    ])
    .unwrap();

    match cli.command {
        Command::Compose(vivarium::cli::ComposeCommand {
            html_body,
            html_body_auto,
            ..
        }) => {
            assert_eq!(html_body, None);
            assert!(html_body_auto);
        }
        other => panic!("unexpected command: {other:?}"),
    }
}

#[test]
fn parses_compose_from() {
    let cli = Cli::try_parse_from([
        "vivi",
        "compose",
        "--from",
        "Alias <alias@example.com>",
        "--to",
        "a@example.com",
        "--subject",
        "hello",
    ])
    .unwrap();

    match cli.command {
        Command::Compose(vivarium::cli::ComposeCommand { from, .. }) => {
            assert_eq!(from.as_deref(), Some("Alias <alias@example.com>"));
        }
        other => panic!("unexpected command: {other:?}"),
    }
}

#[test]
fn parses_want_list_status_all() {
    let cli =
        Cli::try_parse_from(["vivi", "want", "list", "--for", "ceo", "--status", "all"]).unwrap();

    match cli.command {
        Command::Want {
            command: WantCommand::List { status, .. },
        } => assert!(matches!(status, vivarium::cli::WantStatus::All)),
        other => panic!("unexpected command: {other:?}"),
    }
}

#[test]
fn parses_want_done_and_drop() {
    let done = Cli::try_parse_from(["vivi", "want", "done", "abc123", "--for", "ceo"]).unwrap();
    match done.command {
        Command::Want {
            command:
                WantCommand::Done {
                    handle,
                    for_identity,
                    ..
                },
        } => {
            assert_eq!(handle, "abc123");
            assert_eq!(for_identity, "ceo");
        }
        other => panic!("unexpected command: {other:?}"),
    }

    let drop = Cli::try_parse_from(["vivi", "want", "drop", "abc123", "--for", "ceo"]).unwrap();
    match drop.command {
        Command::Want {
            command:
                WantCommand::Drop {
                    handle,
                    for_identity,
                    ..
                },
        } => {
            assert_eq!(handle, "abc123");
            assert_eq!(for_identity, "ceo");
        }
        other => panic!("unexpected command: {other:?}"),
    }
}

#[test]
fn rejects_compose_html_body_auto_without_body() {
    let err = Cli::try_parse_from([
        "vivi",
        "compose",
        "--to",
        "a@example.com",
        "--subject",
        "hello",
        "--html-body-auto",
    ])
    .unwrap_err();

    assert_eq!(err.kind(), clap::error::ErrorKind::MissingRequiredArgument);
}

#[test]
fn parses_default_reply_command() {
    let cli = Cli::try_parse_from([
        "vivi",
        "reply",
        "handle-1",
        "--body",
        "thanks",
        "--append-remote",
    ])
    .unwrap();

    match cli.command {
        Command::Reply(vivarium::cli::ReplyCommand {
            handle,
            from,
            body,
            html_body,
            html_body_auto,
            append_remote,
        }) => {
            assert_eq!(handle, "handle-1");
            assert_eq!(from, None);
            assert_eq!(body.as_deref(), Some("thanks"));
            assert_eq!(html_body, None);
            assert!(!html_body_auto);
            assert!(append_remote);
        }
        other => panic!("unexpected command: {other:?}"),
    }
}

#[test]
fn rejects_reply_html_body_and_auto_together() {
    let err = Cli::try_parse_from([
        "vivi",
        "reply",
        "handle-1",
        "--body",
        "thanks",
        "--html-body",
        "<p>thanks</p>",
        "--html-body-auto",
    ])
    .unwrap_err();

    assert_eq!(err.kind(), clap::error::ErrorKind::ArgumentConflict);
}

#[test]
fn rejects_multiple_exec_flag_modes() {
    let err =
        Cli::try_parse_from(["vivi", "exec", "flag", "abc123", "--read", "--unread"]).unwrap_err();

    assert_eq!(err.kind(), clap::error::ErrorKind::ArgumentConflict);
}

#[test]
fn parses_enqueue_archive() {
    let cli = Cli::try_parse_from(["vivi", "enqueue", "archive", "handle-1"]).unwrap();

    match cli.command {
        Command::Enqueue {
            command: EnqueueCommand::Archive { handles },
        } => {
            assert_eq!(handles, vec!["handle-1"]);
        }
        other => panic!("unexpected command: {other:?}"),
    }
}

#[test]
fn parses_enqueue_archive_batch() {
    let cli = Cli::try_parse_from(["vivi", "enqueue", "archive", "one", "two"]).unwrap();

    match cli.command {
        Command::Enqueue {
            command: EnqueueCommand::Archive { handles },
        } => {
            assert_eq!(handles, vec!["one", "two"]);
        }
        other => panic!("unexpected command: {other:?}"),
    }
}

#[test]
fn rejects_unknown_agent_subcommand() {
    let err = Cli::try_parse_from(["vivi", "agent", "bogus"]).unwrap_err();

    assert_eq!(err.kind(), clap::error::ErrorKind::InvalidSubcommand);
}

#[test]
fn parses_agent_poll_defaults() {
    let cli = Cli::try_parse_from(["vivi", "agent", "poll", "--from", "ian@example.com"]).unwrap();

    match cli.command {
        Command::Agent {
            command:
                AgentCommand::Poll {
                    from_addr,
                    folder,
                    dry_run,
                    json,
                    codex_command,
                    codex_args,
                },
        } => {
            assert_eq!(from_addr, "ian@example.com");
            assert_eq!(folder, "inbox");
            assert!(!dry_run);
            assert!(!json);
            assert_eq!(codex_command, "codex");
            assert_eq!(codex_args, vec!["exec", "-"]);
        }
        other => panic!("unexpected command: {other:?}"),
    }
}

#[test]
fn parses_agent_archive_plan() {
    let cli = Cli::try_parse_from(["vivi", "agent", "archive", "one", "two"]).unwrap();

    match cli.command {
        Command::Agent {
            command:
                AgentCommand::Archive {
                    handles,
                    execute,
                    json,
                },
        } => {
            assert_eq!(handles, vec!["one", "two"]);
            assert!(!execute);
            assert!(!json);
        }
        other => panic!("unexpected command: {other:?}"),
    }
}

#[test]
fn parses_agent_archive_execute() {
    let cli =
        Cli::try_parse_from(["vivi", "agent", "archive", "one", "--execute", "--json"]).unwrap();

    match cli.command {
        Command::Agent {
            command:
                AgentCommand::Archive {
                    handles,
                    execute,
                    json,
                },
        } => {
            assert_eq!(handles, vec!["one"]);
            assert!(execute);
            assert!(json);
        }
        other => panic!("unexpected command: {other:?}"),
    }
}

#[test]
fn parses_agent_delete_execute() {
    let cli = Cli::try_parse_from([
        "vivi",
        "agent",
        "delete",
        "one",
        "two",
        "--expunge",
        "--confirm",
        "--execute",
    ])
    .unwrap();

    match cli.command {
        Command::Agent {
            command:
                AgentCommand::Delete {
                    handles,
                    expunge,
                    confirm,
                    execute,
                    ..
                },
        } => {
            assert_eq!(handles, vec!["one", "two"]);
            assert!(expunge);
            assert!(confirm);
            assert!(execute);
        }
        other => panic!("unexpected command: {other:?}"),
    }
}

#[test]
fn parses_agent_move_and_flag() {
    let cli = Cli::try_parse_from(["vivi", "agent", "move", "h", "trash", "--execute"]).unwrap();

    match cli.command {
        Command::Agent {
            command:
                AgentCommand::Move {
                    handle,
                    folder,
                    execute,
                    ..
                },
        } => {
            assert_eq!(handle, "h");
            assert_eq!(folder, "trash");
            assert!(execute);
        }
        other => panic!("unexpected command: {other:?}"),
    }

    let cli = Cli::try_parse_from([
        "vivi",
        "agent",
        "flag",
        "h",
        "--star",
        "--execute",
        "--json",
    ])
    .unwrap();

    match cli.command {
        Command::Agent {
            command:
                AgentCommand::Flag {
                    handle,
                    read,
                    unread,
                    star,
                    unstar,
                    execute,
                    json,
                },
        } => {
            assert_eq!(handle, "h");
            assert!(star);
            assert!(!read);
            assert!(!unread);
            assert!(!unstar);
            assert!(execute);
            assert!(json);
        }
        other => panic!("unexpected command: {other:?}"),
    }
}

#[test]
fn parses_enqueue_delete_batch_expunge() {
    let cli =
        Cli::try_parse_from(["vivi", "enqueue", "delete", "one", "two", "--expunge"]).unwrap();

    match cli.command {
        Command::Enqueue {
            command: EnqueueCommand::Delete {
                handles, expunge, ..
            },
        } => {
            assert_eq!(handles, vec!["one", "two"]);
            assert!(expunge);
        }
        other => panic!("unexpected command: {other:?}"),
    }
}

#[test]
fn parses_enqueue_send() {
    let cli = Cli::try_parse_from(["vivi", "enqueue", "send", "draft.eml"]).unwrap();

    match cli.command {
        Command::Enqueue {
            command: EnqueueCommand::Send { path, from },
        } => {
            assert_eq!(path, PathBuf::from("draft.eml"));
            assert_eq!(from, None);
        }
        other => panic!("unexpected command: {other:?}"),
    }
}

#[test]
fn parses_enqueue_send_from() {
    let cli = Cli::try_parse_from([
        "vivi",
        "enqueue",
        "send",
        "draft.eml",
        "--from",
        "alias@example.com",
    ])
    .unwrap();

    match cli.command {
        Command::Enqueue {
            command: EnqueueCommand::Send { path, from },
        } => {
            assert_eq!(path, PathBuf::from("draft.eml"));
            assert_eq!(from.as_deref(), Some("alias@example.com"));
        }
        other => panic!("unexpected command: {other:?}"),
    }
}

#[test]
fn parses_enqueue_reply_body() {
    let cli =
        Cli::try_parse_from(["vivi", "enqueue", "reply", "handle-1", "--body", "thanks"]).unwrap();

    match cli.command {
        Command::Enqueue {
            command: EnqueueCommand::Reply { handle, body },
        } => {
            assert_eq!(handle, "handle-1");
            assert_eq!(body, "thanks");
        }
        other => panic!("unexpected command: {other:?}"),
    }
}

#[test]
fn parses_exec_archive_json() {
    let cli = Cli::try_parse_from(["vivi", "exec", "archive", "one", "--json"]).unwrap();

    match cli.command {
        Command::Exec {
            command: ExecCommand::Archive { handles, json },
        } => {
            assert_eq!(handles, vec!["one"]);
            assert!(json);
        }
        other => panic!("unexpected command: {other:?}"),
    }
}

#[test]
fn parses_exec_delete_expunge_confirm() {
    let cli = Cli::try_parse_from([
        "vivi",
        "exec",
        "delete",
        "abc123",
        "def456",
        "--expunge",
        "--confirm",
    ])
    .unwrap();

    match cli.command {
        Command::Exec {
            command:
                ExecCommand::Delete {
                    handles,
                    expunge,
                    confirm,
                    ..
                },
        } => {
            assert_eq!(handles, vec!["abc123", "def456"]);
            assert!(expunge);
            assert!(confirm);
        }
        other => panic!("unexpected command: {other:?}"),
    }
}

#[test]
fn parses_exec_send() {
    let cli = Cli::try_parse_from(["vivi", "exec", "send", "message.eml"]).unwrap();

    match cli.command {
        Command::Exec {
            command: ExecCommand::Send { path, from },
        } => {
            assert_eq!(path, PathBuf::from("message.eml"));
            assert_eq!(from, None);
        }
        other => panic!("unexpected command: {other:?}"),
    }
}

#[test]
fn parses_exec_send_from() {
    let cli = Cli::try_parse_from([
        "vivi",
        "exec",
        "send",
        "message.eml",
        "--from",
        "alias@example.com",
    ])
    .unwrap();

    match cli.command {
        Command::Exec {
            command: ExecCommand::Send { path, from },
        } => {
            assert_eq!(path, PathBuf::from("message.eml"));
            assert_eq!(from.as_deref(), Some("alias@example.com"));
        }
        other => panic!("unexpected command: {other:?}"),
    }
}

#[test]
fn rejects_removed_top_level_write_surface() {
    let err = Cli::try_parse_from(["vivi", "archive", "abc123"]).unwrap_err();

    assert_eq!(err.kind(), clap::error::ErrorKind::InvalidSubcommand);
}

#[test]
fn parses_queue_run_all() {
    let cli = Cli::try_parse_from(["vivi", "queue", "run", "--all"]).unwrap();

    match cli.command {
        Command::Queue {
            command: QueueCommand::Run { ids, all },
        } => {
            assert!(ids.is_empty());
            assert!(all);
        }
        other => panic!("unexpected command: {other:?}"),
    }
}

#[test]
fn parses_labels_json() {
    let cli = Cli::try_parse_from(["vivi", "labels", "--json"]).unwrap();

    match cli.command {
        Command::Labels { json } => assert!(json),
        other => panic!("unexpected command: {other:?}"),
    }
}

#[test]
fn parses_index_rebuild() {
    let cli = Cli::try_parse_from(["vivi", "index", "rebuild"]).unwrap();

    match cli.command {
        Command::Index {
            command: IndexCommand::Rebuild,
        } => {}
        other => panic!("unexpected command: {other:?}"),
    }
}

#[test]
fn parses_index_embeddings_pending_limit() {
    let cli = Cli::try_parse_from([
        "vivi",
        "index",
        "embeddings",
        "--pending",
        "--limit",
        "2",
        "--provider",
        "ollama",
        "--model",
        "mail-embedding-model",
    ])
    .unwrap();

    match cli.command {
        Command::Index {
            command:
                IndexCommand::Embeddings {
                    pending,
                    rebuild,
                    limit,
                    provider,
                    model,
                    endpoint,
                    ..
                },
        } => {
            assert!(pending);
            assert!(!rebuild);
            assert_eq!(limit, Some(2));
            assert_eq!(provider.as_deref(), Some("ollama"));
            assert_eq!(model.as_deref(), Some("mail-embedding-model"));
            assert_eq!(endpoint, None);
        }
        other => panic!("unexpected command: {other:?}"),
    }
}

#[test]
fn parses_semantic_and_hybrid_search_flags() {
    let semantic = Cli::try_parse_from(["vivi", "search", "hello", "--semantic"]).unwrap();
    let hybrid = Cli::try_parse_from(["vivi", "search", "hello", "--hybrid"]).unwrap();

    match semantic.command {
        Command::Search {
            semantic, hybrid, ..
        } => {
            assert!(semantic);
            assert!(!hybrid);
        }
        other => panic!("unexpected command: {other:?}"),
    }
    match hybrid.command {
        Command::Search {
            semantic, hybrid, ..
        } => {
            assert!(!semantic);
            assert!(hybrid);
        }
        other => panic!("unexpected command: {other:?}"),
    }
}

#[test]
fn parses_search_count_and_folder() {
    let cli = Cli::try_parse_from(["vivi", "search", "DoorDash", "--folder", "inbox", "--count"])
        .unwrap();

    match cli.command {
        Command::Search {
            query,
            folder,
            count,
            ..
        } => {
            assert_eq!(query, "DoorDash");
            assert_eq!(folder.as_deref(), Some("inbox"));
            assert!(count);
        }
        other => panic!("unexpected command: {other:?}"),
    }
}

#[test]
fn parses_search_sender_filters() {
    let cli = Cli::try_parse_from([
        "vivi",
        "search",
        "invoice",
        "--from",
        "person@example.com",
        "--from-domain",
        "example.com",
    ])
    .unwrap();

    match cli.command {
        Command::Search {
            from_addr,
            from_domain,
            ..
        } => {
            assert_eq!(from_addr.as_deref(), Some("person@example.com"));
            assert_eq!(from_domain.as_deref(), Some("example.com"));
        }
        other => panic!("unexpected command: {other:?}"),
    }
}

#[test]
fn parses_label_add_dry_run_json() {
    let cli = Cli::try_parse_from([
        "vivi",
        "label",
        "handle-1",
        "--add",
        "Work",
        "--dry-run",
        "--json",
    ])
    .unwrap();

    match cli.command {
        Command::Label {
            handle,
            add,
            remove,
            dry_run,
            json,
        } => {
            assert_eq!(handle, "handle-1");
            assert_eq!(add.as_deref(), Some("Work"));
            assert!(remove.is_none());
            assert!(dry_run);
            assert!(json);
        }
        other => panic!("unexpected command: {other:?}"),
    }
}

#[test]
fn parses_mail_absorb() {
    let cli = Cli::try_parse_from([
        "vivi", "mail", "absorb", "abc123", "--for", "mind", "--note", "handled",
    ])
    .unwrap();

    match cli.command {
        Command::Mail {
            command:
                MailCommand::Absorb {
                    handle,
                    for_identity,
                    note,
                    ..
                },
        } => {
            assert_eq!(handle, "abc123");
            assert_eq!(for_identity, "mind");
            assert_eq!(note.as_deref(), Some("handled"));
        }
        other => panic!("unexpected command: {other:?}"),
    }
}

#[test]
fn parses_task_from_source_handle() {
    let cli = Cli::try_parse_from([
        "vivi",
        "task",
        "from",
        "abc123",
        "--for",
        "mind",
        "--to",
        "hand-2",
        "--subject",
        "Do work",
        "--body",
        "body",
    ])
    .unwrap();

    match cli.command {
        Command::Task {
            command: TaskCommand::From(command),
        } => {
            assert_eq!(command.handle, "abc123");
            assert_eq!(command.for_identity, "mind");
            assert_eq!(command.to, vec!["hand-2"]);
        }
        other => panic!("unexpected command: {other:?}"),
    }
}

#[test]
fn parses_want_set_priority() {
    let cli = Cli::try_parse_from([
        "vivi",
        "want",
        "set-priority",
        "abc123",
        "--for",
        "mind",
        "--priority",
        "P1",
        "--rank",
        "20",
        "--repo",
        "faber-runtime",
        "--lane",
        "correctness",
    ])
    .unwrap();

    match cli.command {
        Command::Want {
            command:
                WantCommand::SetPriority {
                    handle,
                    priority,
                    rank,
                    repo,
                    lane,
                    ..
                },
        } => {
            assert_eq!(handle, "abc123");
            assert_eq!(priority, "P1");
            assert_eq!(rank, Some(20));
            assert_eq!(repo.as_deref(), Some("faber-runtime"));
            assert_eq!(lane.as_deref(), Some("correctness"));
        }
        other => panic!("unexpected command: {other:?}"),
    }
}

#[test]
fn parses_cycle_intake() {
    let cli = Cli::try_parse_from([
        "vivi",
        "cycle",
        "intake",
        "--for",
        "mind",
        "--cursor-file",
        ".vivi/mind.cursor",
        "--write-cursor",
        "--json",
    ])
    .unwrap();

    match cli.command {
        Command::Cycle {
            command:
                CycleCommand::Intake {
                    for_identity,
                    cursor_file,
                    write_cursor,
                    json,
                    ..
                },
        } => {
            assert_eq!(for_identity, "mind");
            assert_eq!(cursor_file, Some(PathBuf::from(".vivi/mind.cursor")));
            assert!(write_cursor);
            assert!(json);
        }
        other => panic!("unexpected command: {other:?}"),
    }
}

#[test]
fn parses_memo_search_command() {
    let cli =
        Cli::try_parse_from(["vivi", "memo", "search", "railway deploy", "--for", "mind"]).unwrap();

    match cli.command {
        Command::Memo {
            command:
                MemoCommand::Search {
                    query,
                    for_identity,
                    subject,
                    json,
                    ..
                },
        } => {
            assert_eq!(query, "railway deploy");
            assert_eq!(for_identity, "mind");
            assert!(!subject);
            assert!(!json);
        }
        other => panic!("unexpected command: {other:?}"),
    }
}

#[test]
fn parses_memo_search_subject_only() {
    let cli = Cli::try_parse_from([
        "vivi",
        "memo",
        "search",
        "ACCEPT*",
        "--for",
        "mind",
        "--subject",
        "--json",
    ])
    .unwrap();

    match cli.command {
        Command::Memo {
            command:
                MemoCommand::Search {
                    query,
                    for_identity,
                    subject,
                    json,
                    ..
                },
        } => {
            assert_eq!(query, "ACCEPT*");
            assert_eq!(for_identity, "mind");
            assert!(subject);
            assert!(json);
        }
        other => panic!("unexpected command: {other:?}"),
    }
}

#[test]
fn parses_task_send_with_depends_on() {
    let cli = Cli::try_parse_from([
        "vivi",
        "task",
        "send",
        "--from",
        "ceo",
        "--to",
        "hand-1",
        "--subject",
        "do this after that",
        "--body",
        "depends on prior work",
        "--depends-on",
        "abc123",
        "--depends-on",
        "def456",
    ])
    .unwrap();

    match cli.command {
        Command::Task {
            command:
                TaskCommand::Send(TaskSendCommand {
                    send, depends_on, ..
                }),
        } => {
            assert_eq!(send.from, "ceo");
            assert_eq!(send.to, vec!["hand-1"]);
            assert_eq!(send.subject, "do this after that");
            assert_eq!(depends_on, vec!["abc123", "def456"]);
        }
        other => panic!("unexpected command: {other:?}"),
    }
}

#[test]
fn parses_task_list_blocked_and_blocking() {
    let cli = Cli::try_parse_from([
        "vivi",
        "task",
        "list",
        "--for",
        "hand-1",
        "--blocked",
        "--blocking",
        "abc123",
    ])
    .unwrap();

    match cli.command {
        Command::Task {
            command:
                TaskCommand::List {
                    for_identity,
                    blocked,
                    blocking,
                    ..
                },
        } => {
            assert_eq!(for_identity, "hand-1");
            assert!(blocked);
            assert_eq!(blocking.as_deref(), Some("abc123"));
        }
        other => panic!("unexpected command: {other:?}"),
    }
}

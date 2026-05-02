use std::path::PathBuf;

use clap::Parser;
use vivarium::cli::{AgentCommand, Cli, Command, IndexCommand};

#[test]
fn parses_archive_dry_run_json() {
    let cli = Cli::try_parse_from(["vivi", "archive", "abc123", "--dry-run", "--json"]).unwrap();

    match cli.command {
        Command::Archive {
            handles,
            dry_run,
            json,
        } => {
            assert_eq!(handles, vec!["abc123"]);
            assert!(dry_run);
            assert!(json);
        }
        other => panic!("unexpected command: {other:?}"),
    }
}

#[test]
fn parses_delete_expunge_confirm() {
    let cli = Cli::try_parse_from(["vivi", "delete", "abc123", "--expunge", "--confirm"]).unwrap();

    match cli.command {
        Command::Delete {
            handle,
            expunge,
            confirm,
            ..
        } => {
            assert_eq!(handle, "abc123");
            assert!(expunge);
            assert!(confirm);
        }
        other => panic!("unexpected command: {other:?}"),
    }
}

#[test]
fn parses_default_send_command() {
    let cli = Cli::try_parse_from(["vivi", "send", "message.eml"]).unwrap();

    match cli.command {
        Command::Send { path } => assert_eq!(path, PathBuf::from("message.eml")),
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
        Command::Compose {
            to,
            cc,
            bcc,
            subject,
            body,
            append_remote,
        } => {
            assert_eq!(to, vec!["a@example.com"]);
            assert_eq!(cc, vec!["b@example.com"]);
            assert_eq!(bcc, vec!["c@example.com"]);
            assert_eq!(subject, "hello");
            assert_eq!(body.as_deref(), Some("body"));
            assert!(append_remote);
        }
        other => panic!("unexpected command: {other:?}"),
    }
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
        Command::Reply {
            handle,
            body,
            append_remote,
        } => {
            assert_eq!(handle, "handle-1");
            assert_eq!(body.as_deref(), Some("thanks"));
            assert!(append_remote);
        }
        other => panic!("unexpected command: {other:?}"),
    }
}

#[test]
fn rejects_multiple_flag_modes() {
    let err = Cli::try_parse_from(["vivi", "flag", "abc123", "--read", "--unread"]).unwrap_err();

    assert_eq!(err.kind(), clap::error::ErrorKind::ArgumentConflict);
}

#[test]
fn parses_agent_archive_plan_by_default() {
    let cli = Cli::try_parse_from(["vivi", "agent", "archive", "handle-1"]).unwrap();

    match cli.command {
        Command::Agent {
            command: AgentCommand::Archive { handle, execute },
        } => {
            assert_eq!(handle, "handle-1");
            assert!(!execute);
        }
        other => panic!("unexpected command: {other:?}"),
    }
}

#[test]
fn parses_agent_send_execute() {
    let cli = Cli::try_parse_from(["vivi", "agent", "send", "draft.eml", "--execute"]).unwrap();

    match cli.command {
        Command::Agent {
            command: AgentCommand::Send { path, execute },
        } => {
            assert_eq!(path, PathBuf::from("draft.eml"));
            assert!(execute);
        }
        other => panic!("unexpected command: {other:?}"),
    }
}

#[test]
fn parses_agent_reply_body_plan() {
    let cli =
        Cli::try_parse_from(["vivi", "agent", "reply", "handle-1", "--body", "thanks"]).unwrap();

    match cli.command {
        Command::Agent {
            command:
                AgentCommand::Reply {
                    handle,
                    body,
                    execute,
                },
        } => {
            assert_eq!(handle, "handle-1");
            assert_eq!(body, "thanks");
            assert!(!execute);
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

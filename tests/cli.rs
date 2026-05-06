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
    let cli = Cli::try_parse_from([
        "vivi",
        "delete",
        "abc123",
        "def456",
        "--expunge",
        "--confirm",
    ])
    .unwrap();

    match cli.command {
        Command::Delete {
            handles,
            expunge,
            confirm,
            ..
        } => {
            assert_eq!(handles, vec!["abc123", "def456"]);
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
fn parses_list_filter() {
    let cli = Cli::try_parse_from(["vivi", "list", "inbox", "--filter", "DoorDash"]).unwrap();

    match cli.command {
        Command::List { folder, filter, .. } => {
            assert_eq!(folder, "inbox");
            assert_eq!(filter.as_deref(), Some("DoorDash"));
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
            subject,
            body,
            html_body,
            html_body_auto,
            append_remote,
        }) => {
            assert_eq!(to, vec!["a@example.com"]);
            assert_eq!(cc, vec!["b@example.com"]);
            assert_eq!(bcc, vec!["c@example.com"]);
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
            body,
            html_body,
            html_body_auto,
            append_remote,
        }) => {
            assert_eq!(handle, "handle-1");
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
fn rejects_multiple_flag_modes() {
    let err = Cli::try_parse_from(["vivi", "flag", "abc123", "--read", "--unread"]).unwrap_err();

    assert_eq!(err.kind(), clap::error::ErrorKind::ArgumentConflict);
}

#[test]
fn parses_agent_archive_plan_by_default() {
    let cli = Cli::try_parse_from(["vivi", "agent", "archive", "handle-1"]).unwrap();

    match cli.command {
        Command::Agent {
            command: AgentCommand::Archive { handles },
        } => {
            assert_eq!(handles, vec!["handle-1"]);
        }
        other => panic!("unexpected command: {other:?}"),
    }
}

#[test]
fn parses_agent_archive_batch() {
    let cli = Cli::try_parse_from(["vivi", "agent", "archive", "one", "two"]).unwrap();

    match cli.command {
        Command::Agent {
            command: AgentCommand::Archive { handles },
        } => {
            assert_eq!(handles, vec!["one", "two"]);
        }
        other => panic!("unexpected command: {other:?}"),
    }
}

#[test]
fn rejects_agent_execute_flag() {
    let err = Cli::try_parse_from(["vivi", "agent", "archive", "one", "--execute"]).unwrap_err();

    assert_eq!(err.kind(), clap::error::ErrorKind::UnknownArgument);
}

#[test]
fn parses_agent_delete_batch_expunge_plan() {
    let cli = Cli::try_parse_from(["vivi", "agent", "delete", "one", "two", "--expunge"]).unwrap();

    match cli.command {
        Command::Agent {
            command: AgentCommand::Delete { handles, expunge },
        } => {
            assert_eq!(handles, vec!["one", "two"]);
            assert!(expunge);
        }
        other => panic!("unexpected command: {other:?}"),
    }
}

#[test]
fn parses_agent_send_plan() {
    let cli = Cli::try_parse_from(["vivi", "agent", "send", "draft.eml"]).unwrap();

    match cli.command {
        Command::Agent {
            command: AgentCommand::Send { path },
        } => {
            assert_eq!(path, PathBuf::from("draft.eml"));
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
            command: AgentCommand::Reply { handle, body },
        } => {
            assert_eq!(handle, "handle-1");
            assert_eq!(body, "thanks");
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
        "cassio-embedding",
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
                    ..
                },
        } => {
            assert!(pending);
            assert!(!rebuild);
            assert_eq!(limit, Some(2));
            assert_eq!(provider, "ollama");
            assert_eq!(model, "cassio-embedding");
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

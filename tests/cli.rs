use std::path::PathBuf;

use clap::Parser;
use vivarium::cli::{
    AgentCommand, Cli, Command, EnqueueCommand, ExecCommand, IndexCommand, ProtonCommand,
    QueueCommand,
};

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
            json,
            ..
        } => {
            assert_eq!(folder, "inbox");
            assert_eq!(filter.as_deref(), Some("DoorDash"));
            assert!(unread);
            assert!(!read);
            assert!(json);
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
    let err = Cli::try_parse_from(["vivi", "agent", "archive", "one"]).unwrap_err();

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

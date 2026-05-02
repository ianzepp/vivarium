use super::*;
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpListener;
use tokio::sync::Mutex;

#[test]
fn plans_uid_move_when_supported() {
    let plan = plan_move(
        MutationTarget::Archive("Archive".into()),
        &MutationCapabilities {
            move_supported: true,
            uidplus: true,
        },
    )
    .unwrap();

    assert_eq!(
        plan,
        MutationPlan::Move {
            destination: "Archive".into(),
            command_path: CommandPath::UidMove
        }
    );
}

#[test]
fn plans_copy_delete_uid_expunge_fallback_only_with_uidplus() {
    let plan = plan_move(
        MutationTarget::Trash("Trash".into()),
        &MutationCapabilities {
            move_supported: false,
            uidplus: true,
        },
    )
    .unwrap();

    assert_eq!(
        plan,
        MutationPlan::Move {
            destination: "Trash".into(),
            command_path: CommandPath::CopyStoreDeletedUidExpunge
        }
    );
}

#[test]
fn refuses_unsafe_move_fallback_without_uidplus() {
    let err = plan_move(
        MutationTarget::Archive("Archive".into()),
        &MutationCapabilities {
            move_supported: false,
            uidplus: false,
        },
    )
    .unwrap_err();

    assert!(err.to_string().contains("refusing unsafe expunge"));
}

#[test]
fn plans_flag_store_queries() {
    assert_eq!(
        plan_flag(FlagMutation::Read),
        MutationPlan::Flag {
            mutation: FlagMutation::Read,
            store_query: "+FLAGS.SILENT (\\Seen)"
        }
    );
    assert_eq!(
        plan_flag(FlagMutation::Unstarred),
        MutationPlan::Flag {
            mutation: FlagMutation::Unstarred,
            store_query: "-FLAGS.SILENT (\\Flagged)"
        }
    );
}

#[test]
fn rejects_stale_uidvalidity() {
    let remote = remote_identity();
    let err = verify_uidvalidity(&remote, Some(8)).unwrap_err();

    assert!(err.to_string().contains("stale remote reference"));
}

#[tokio::test]
async fn mock_imap_executes_flag_mutations() {
    let server = MockImapServer::start(7).await;
    let result = execute_mock_plan(&server, plan_flag(FlagMutation::Read))
        .await
        .unwrap();

    assert_eq!(result.command_path, "UID STORE");
    assert_eq!(result.reconciliation, ReconciliationAction::RefreshFlags);
    assert!(
        server
            .commands()
            .await
            .iter()
            .any(|command| { command.contains("UID STORE 42 +FLAGS.SILENT (\\Seen)") })
    );
}

#[tokio::test]
async fn mock_imap_executes_uid_move() {
    let server = MockImapServer::start(7).await;
    let plan = plan_move(
        MutationTarget::Archive("Archive".into()),
        &MutationCapabilities {
            move_supported: true,
            uidplus: true,
        },
    )
    .unwrap();
    let result = execute_mock_plan(&server, plan).await.unwrap();

    assert_eq!(result.destination_mailbox.as_deref(), Some("Archive"));
    assert_eq!(result.command_path, "UID MOVE");
    assert!(
        server
            .commands()
            .await
            .iter()
            .any(|command| command.contains("UID MOVE 42") && command.contains("Archive"))
    );
}

#[tokio::test]
async fn mock_imap_executes_copy_store_expunge_fallback() {
    let server = MockImapServer::start(7).await;
    let plan = plan_move(
        MutationTarget::Trash("Trash".into()),
        &MutationCapabilities {
            move_supported: false,
            uidplus: true,
        },
    )
    .unwrap();
    let result = execute_mock_plan(&server, plan).await.unwrap();
    let commands = server.commands().await.join("\n");

    assert_eq!(result.command_path, "UID COPY + UID STORE + UID EXPUNGE");
    assert!(commands.contains("UID COPY 42 Trash"));
    assert!(commands.contains("UID STORE 42 +FLAGS.SILENT (\\Deleted)"));
    assert!(commands.contains("UID EXPUNGE 42"));
}

#[tokio::test]
async fn mock_imap_rejects_stale_uidvalidity_before_write() {
    let server = MockImapServer::start(8).await;
    let err = execute_mock_plan(&server, plan_flag(FlagMutation::Read))
        .await
        .unwrap_err();
    let commands = server.commands().await.join("\n");

    assert!(err.to_string().contains("stale remote reference"));
    assert!(!commands.contains("UID STORE"));
}

fn remote_identity() -> RemoteIdentity {
    RemoteIdentity {
        account: "acct".into(),
        provider: "protonmail".into(),
        remote_mailbox: "INBOX".into(),
        local_folder: "inbox".into(),
        uid: 42,
        uidvalidity: 7,
        rfc_message_id: "m@example.com".into(),
        size: 123,
        content_fingerprint: "abc".into(),
    }
}

async fn execute_mock_plan(
    server: &MockImapServer,
    plan: MutationPlan,
) -> Result<MutationResult, VivariumError> {
    let tcp = tokio::net::TcpStream::connect(("127.0.0.1", server.port))
        .await
        .unwrap();
    let client = async_imap::Client::new(tcp);
    let mut session = client
        .login("mock@example.com", "secret")
        .await
        .map_err(|(e, _)| VivariumError::Imap(format!("mock login failed: {e}")))?;
    let remote = remote_identity();
    let mailbox = session
        .select(&remote.remote_mailbox)
        .await
        .map_err(|e| VivariumError::Imap(format!("mock select failed: {e}")))?;
    verify_uidvalidity(&remote, mailbox.uid_validity)?;
    let result = execute_selected(&mut session, &remote, plan).await;
    session.logout().await.ok();
    result
}

struct MockImapServer {
    port: u16,
    commands: Arc<Mutex<Vec<String>>>,
}

impl MockImapServer {
    async fn start(uidvalidity: u32) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let commands = Arc::new(Mutex::new(Vec::new()));
        let server_commands = Arc::clone(&commands);
        tokio::spawn(async move {
            let (tcp, _) = listener.accept().await.unwrap();
            serve_connection(tcp, uidvalidity, server_commands).await;
        });
        Self { port, commands }
    }

    async fn commands(&self) -> Vec<String> {
        self.commands.lock().await.clone()
    }
}

async fn serve_connection(
    stream: tokio::net::TcpStream,
    uidvalidity: u32,
    commands: Arc<Mutex<Vec<String>>>,
) {
    let (read, mut write) = tokio::io::split(stream);
    let mut lines = BufReader::new(read).lines();
    write.write_all(b"* OK mock imap ready\r\n").await.unwrap();

    while let Some(line) = lines.next_line().await.unwrap() {
        commands.lock().await.push(line.clone());
        let tag = line.split_whitespace().next().unwrap_or("A0");
        let upper = line.to_ascii_uppercase();
        if upper.contains(" LOGIN ") {
            write_ok(&mut write, tag, "LOGIN completed").await;
        } else if upper.contains(" SELECT ") {
            select_ok(&mut write, tag, uidvalidity).await;
        } else if upper.contains("UID STORE") {
            write_ok(&mut write, tag, "STORE completed").await;
        } else if upper.contains("UID MOVE") {
            write_ok(&mut write, tag, "MOVE completed").await;
        } else if upper.contains("UID COPY") {
            write_ok(&mut write, tag, "COPY completed").await;
        } else if upper.contains("UID EXPUNGE") {
            write_ok(&mut write, tag, "EXPUNGE completed").await;
        } else if upper.contains(" LOGOUT") {
            write.write_all(b"* BYE logging out\r\n").await.unwrap();
            write_ok(&mut write, tag, "LOGOUT completed").await;
            break;
        } else {
            write_ok(&mut write, tag, "OK").await;
        }
    }
}

async fn select_ok<W: tokio::io::AsyncWrite + Unpin>(write: &mut W, tag: &str, uidvalidity: u32) {
    write.write_all(b"* 1 EXISTS\r\n").await.unwrap();
    write
        .write_all(format!("* OK [UIDVALIDITY {uidvalidity}] UIDs valid\r\n").as_bytes())
        .await
        .unwrap();
    write_ok(write, tag, "[READ-WRITE] SELECT completed").await;
}

async fn write_ok<W: tokio::io::AsyncWrite + Unpin>(write: &mut W, tag: &str, text: &str) {
    write
        .write_all(format!("{tag} OK {text}\r\n").as_bytes())
        .await
        .unwrap();
}

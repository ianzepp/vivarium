use super::*;
use crate::mailspace::{GraphNodeView, SendRequest};
use crate::storage::Storage;

fn roster() -> (Mailspace, tempfile::TempDir) {
    let tmp = tempfile::tempdir().unwrap();
    let mut mailspace = Mailspace::init(Some(tmp.path())).unwrap();
    mailspace.add_identity("ceo").unwrap();
    mailspace.add_identity("cto").unwrap();
    (mailspace, tmp)
}

fn send_item(
    mailspace: &Mailspace,
    role: &str,
    kind: Option<&str>,
    subject: &str,
    depends_on: Vec<String>,
) -> String {
    mailspace
        .send(SendRequest {
            from: "ceo".into(),
            to: vec!["cto".into()],
            cc: Vec::new(),
            bcc: Vec::new(),
            subject: subject.into(),
            body: "body".into(),
            role: role.into(),
            kind: kind.map(str::to_string),
            reply_to: None,
            depends_on,
        })
        .unwrap()
        .delivered
        .remove(0)
        .handle
}

fn node(mailspace: &Mailspace, source_id: &str) -> GraphNodeView {
    mailspace
        .graph_show("backlog")
        .unwrap()
        .nodes
        .into_iter()
        .find(|n| n.source_id == source_id)
        .unwrap()
}

#[test]
fn attach_mints_backlog_node_ready() {
    let (mailspace, _tmp) = roster();
    mailspace
        .backlog_attach("want", "w1handle0", "extract retry helper", &[])
        .unwrap();
    let view = node(&mailspace, "w1handle0");
    assert_eq!(view.state, "open");
    assert_eq!(view.readiness, "ready");
    assert_eq!(view.label, "extract retry helper");
}

#[test]
fn attach_is_idempotent() {
    let (mailspace, _tmp) = roster();
    mailspace
        .backlog_attach("need", "n1handle0", "fix auth", &[])
        .unwrap();
    mailspace
        .backlog_attach("need", "n1handle0", "fix auth", &[])
        .unwrap();
    assert_eq!(mailspace.graph_show("backlog").unwrap().nodes.len(), 1);
}

#[test]
fn dependency_blocks_until_completed() {
    let (mailspace, _tmp) = roster();
    let dep = send_item(&mailspace, "needs", Some("need"), "auth fix", Vec::new());
    mailspace
        .backlog_attach("need", &dep, "auth fix", &[])
        .unwrap();

    mailspace
        .backlog_attach("want", "w2handle0", "retry helper", &[dep.clone()])
        .unwrap();
    assert_eq!(node(&mailspace, "w2handle0").readiness, "blocked");
    assert_eq!(node(&mailspace, &dep).readiness, "ready");

    mailspace.backlog_complete_item(&dep).unwrap();
    assert_eq!(node(&mailspace, &dep).state, "done");
    assert_eq!(node(&mailspace, "w2handle0").readiness, "ready");
}

#[test]
fn done_dependency_mints_done_node_and_unlocks() {
    let (mailspace, _tmp) = roster();
    let dep = send_item(&mailspace, "needs", Some("need"), "old fix", Vec::new());
    mailspace
        .move_item("cto", &dep, "done", None, "need done", None)
        .unwrap();

    mailspace
        .backlog_attach("want", "w3handle0", "follow-up", &[dep])
        .unwrap();
    assert_eq!(node(&mailspace, "w3handle0").readiness, "ready");
}

#[test]
fn validate_deps_rejects_unknown_handle() {
    let (mailspace, _tmp) = roster();
    let err = mailspace
        .backlog_validate_deps(&["deadbeef".into()])
        .unwrap_err();
    assert!(err.to_string().contains("deadbeef"), "{err}");
}

#[test]
fn validate_deps_rejects_task_handle() {
    let (mailspace, _tmp) = roster();
    let task = send_item(&mailspace, "tasks", Some("task"), "do work", Vec::new());
    let err = mailspace.backlog_validate_deps(&[task]).unwrap_err();
    assert!(err.to_string().contains("task"), "{err}");
}

#[test]
fn lifecycle_moves_sync_node_state() {
    let (mailspace, _tmp) = roster();
    let need = send_item(&mailspace, "needs", Some("need"), "fix auth", Vec::new());
    mailspace
        .backlog_attach("need", &need, "fix auth", &[])
        .unwrap();

    mailspace
        .move_item("cto", &need, "done", None, "need done", None)
        .unwrap();
    assert_eq!(node(&mailspace, &need).state, "done");

    mailspace
        .move_item("cto", &need, "needs", None, "need reopen", None)
        .unwrap();
    assert_eq!(node(&mailspace, &need).state, "open");
}

#[test]
fn promotion_leaves_node_open() {
    let (mailspace, _tmp) = roster();
    let want = send_item(&mailspace, "wants", Some("want"), "later work", Vec::new());
    mailspace
        .backlog_attach("want", &want, "later work", &[])
        .unwrap();

    mailspace
        .move_item("cto", &want, "needs", None, "want promote", None)
        .unwrap();
    assert_eq!(node(&mailspace, &want).state, "open");
    assert_eq!(node(&mailspace, &want).readiness, "ready");
}

#[test]
fn complete_without_graph_is_noop() {
    let tmp = tempfile::tempdir().unwrap();
    let mailspace = Mailspace::init(Some(tmp.path())).unwrap();
    mailspace.backlog_complete_item("missing0").unwrap();
    mailspace.backlog_reopen_item("missing0").unwrap();
    assert!(
        mailspace
            .storage()
            .unwrap()
            .work_graphs()
            .unwrap()
            .is_empty()
    );
}

#[test]
fn mint_records_backlog_events() {
    let (mailspace, _tmp) = roster();
    mailspace
        .backlog_attach("need", "n9handle0", "audit", &[])
        .unwrap();
    let storage: Storage = mailspace.storage().unwrap();
    let events = storage.list_work_graph_events_after(0).unwrap();
    assert!(
        events
            .iter()
            .any(|e| e.event_type == "backlog_attached" && e.note.as_deref() == Some("kind=need")),
        "{events:?}"
    );
}

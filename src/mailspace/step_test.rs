use super::*;
use crate::mailspace::SendRequest;

fn roster() -> (Mailspace, tempfile::TempDir) {
    let tmp = tempfile::tempdir().unwrap();
    let mut mailspace = Mailspace::init(Some(tmp.path())).unwrap();
    mailspace.add_identity("ceo").unwrap();
    mailspace.add_identity("cto").unwrap();
    (mailspace, tmp)
}

fn send_item(mailspace: &Mailspace, role: &str, kind: &str, subject: &str, body: &str) -> String {
    mailspace
        .send(SendRequest {
            from: "ceo".into(),
            to: vec!["cto".into()],
            cc: Vec::new(),
            bcc: Vec::new(),
            subject: subject.into(),
            body: body.into(),
            role: role.into(),
            kind: Some(kind.into()),
            reply_to: None,
            depends_on: Vec::new(),
        })
        .unwrap()
        .delivered
        .remove(0)
        .handle
}

fn manifest_for(mailspace: &Mailspace, item: &str) -> Option<StepDispatch> {
    mailspace
        .step_shadow()
        .unwrap()
        .dispatches
        .iter()
        .find(|d| d.item == item)
        .cloned()
}

fn exception_for(mailspace: &Mailspace, item: &str) -> Option<StepException> {
    mailspace
        .step_shadow()
        .unwrap()
        .exceptions
        .iter()
        .find(|e| e.item == item)
        .cloned()
}

#[test]
fn step_manifest_empty_without_backlog_graph() {
    let tmp = tempfile::tempdir().unwrap();
    let mailspace = Mailspace::init(Some(tmp.path())).unwrap();
    let manifest = mailspace.step_shadow().unwrap();
    assert!(manifest.dispatches.is_empty());
    assert!(manifest.exceptions.is_empty());
    assert!(manifest.decisions.is_empty());
}

#[test]
fn ready_task_with_done_when_dispatches() {
    let (mailspace, _tmp) = roster();
    let task = send_item(
        &mailspace,
        "tasks",
        "task",
        "unit A",
        "goal: g.md\nunit: U-1\ndone_when: malformed input rejected\nwrite_scope: src/validate.rs",
    );
    let dispatch =
        manifest_for(&mailspace, &task).unwrap_or_else(|| panic!("expected dispatch for {task}"));
    assert_eq!(dispatch.kind, "task");
    assert_eq!(dispatch.done_when_clauses, 1);
    assert!(dispatch.write_scope);
    assert_eq!(dispatch.subject, "unit A");
}

#[test]
fn ready_wants_require_promotion() {
    let (mailspace, _tmp) = roster();
    let want = send_item(
        &mailspace,
        "wants",
        "want",
        "later work",
        "done_when: exists",
    );
    let exception =
        exception_for(&mailspace, &want).unwrap_or_else(|| panic!("expected exception for {want}"));
    assert_eq!(exception.reason, "want_requires_promotion");
}

#[test]
fn task_without_done_when_is_exception() {
    let (mailspace, _tmp) = roster();
    let task = send_item(&mailspace, "tasks", "task", "vague work", "just do it");
    let exception =
        exception_for(&mailspace, &task).unwrap_or_else(|| panic!("expected exception for {task}"));
    assert_eq!(exception.reason, "no_done_when");
}

#[test]
fn lowered_need_awaits_units() {
    let (mailspace, _tmp) = roster();
    let need = send_item(
        &mailspace,
        "needs",
        "need",
        "lower me",
        "done_when: join complete",
    );
    let unit = send_item(
        &mailspace,
        "tasks",
        "task",
        "unit 1",
        "done_when: unit lands",
    );
    mailspace.backlog_bind_units(&need, &[unit]).unwrap();

    let exception =
        exception_for(&mailspace, &need).unwrap_or_else(|| panic!("expected exception for {need}"));
    assert_eq!(exception.reason, "lowered_awaiting_units");
}

#[test]
fn blocked_nodes_do_not_appear() {
    let (mailspace, _tmp) = roster();
    let dep = send_item(&mailspace, "tasks", "task", "first", "done_when: lands");
    // A task depending on the first one stays out of the manifest entirely
    // until its prerequisite completes.
    let result = mailspace
        .send(SendRequest {
            from: "ceo".into(),
            to: vec!["cto".into()],
            cc: Vec::new(),
            bcc: Vec::new(),
            subject: "second".into(),
            body: "done_when: lands".into(),
            role: "tasks".into(),
            kind: Some("task".into()),
            reply_to: None,
            depends_on: vec![dep],
        })
        .unwrap();
    let blocked_item = &result.delivered[0].handle;

    let manifest = mailspace.step_shadow().unwrap();
    assert!(
        !manifest.dispatches.iter().any(|d| d.item == *blocked_item),
        "{manifest:?}"
    );
    assert!(
        !manifest.exceptions.iter().any(|e| e.item == *blocked_item),
        "{manifest:?}"
    );
}

use super::*;

#[test]
fn detects_mailspace_from_subdirectory() {
    let tmp = tempfile::tempdir().unwrap();
    Mailspace::init(Some(tmp.path())).unwrap();
    let deep = tmp.path().join("src/deep");
    fs::create_dir_all(&deep).unwrap();
    let old = std::env::current_dir().unwrap();
    std::env::set_current_dir(&deep).unwrap();
    let found = Mailspace::discover(None).unwrap();
    std::env::set_current_dir(old).unwrap();

    assert_eq!(found.root, tmp.path().canonicalize().unwrap());
    assert!(!deep.join(".vivi").exists());
}

#[test]
fn local_delivery_rejects_unknown_identity() {
    let tmp = tempfile::tempdir().unwrap();
    let mut mailspace = Mailspace::init(Some(tmp.path())).unwrap();
    mailspace.add_identity("ceo").unwrap();

    let err = mailspace
        .send(SendRequest {
            from: "ceo".into(),
            to: vec!["cto".into()],
            cc: Vec::new(),
            bcc: Vec::new(),
            subject: "hello".into(),
            body: "body".into(),
            role: "inbox".into(),
            kind: None,
        })
        .unwrap_err();

    assert!(err.to_string().contains("unknown local identity"));
}

#[test]
fn task_move_keeps_handle_stable() {
    let tmp = tempfile::tempdir().unwrap();
    let mut mailspace = Mailspace::init(Some(tmp.path())).unwrap();
    mailspace.add_identity("ceo").unwrap();
    mailspace.add_identity("cto").unwrap();
    let sent = mailspace
        .send(SendRequest {
            from: "ceo".into(),
            to: vec!["cto".into()],
            cc: Vec::new(),
            bcc: Vec::new(),
            subject: "task".into(),
            body: "do it".into(),
            role: "tasks".into(),
            kind: Some("task".into()),
        })
        .unwrap();
    let handle = sent.delivered[0].handle.clone();

    let done = mailspace.move_task("cto", &handle, "done", None).unwrap();
    let open = mailspace.list("cto", "tasks").unwrap();
    let closed = mailspace.list("cto", "done").unwrap();

    assert_eq!(done, handle);
    assert!(open.is_empty());
    assert_eq!(closed.len(), 1);
    assert_eq!(closed[0].handle, handle);
}

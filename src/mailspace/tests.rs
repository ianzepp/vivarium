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
            reply_to: None,
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
            reply_to: None,
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

#[test]
fn rename_identity_keeps_historical_mail_and_old_alias_working() {
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
            reply_to: None,
        })
        .unwrap();
    let handle = sent.delivered[0].handle.clone();

    let new_address = mailspace.rename_identity("cto", "cro").unwrap();
    assert_eq!(new_address, mailspace.address_for("cro"));
    assert_eq!(mailspace.config.identities.len(), 2);
    let renamed = mailspace
        .config
        .identities
        .iter()
        .find(|identity| identity.name == "cro")
        .unwrap();
    assert_eq!(renamed.aliases, vec!["cto".to_string()]);

    // Historical mail (stored under the old identity name) is untouched
    // but still shows up when listing under the new name.
    let open = mailspace.list("cro", "tasks").unwrap();
    assert_eq!(open.len(), 1);
    assert_eq!(open[0].account, "cto");

    // The old name still resolves for future commands.
    assert_eq!(mailspace.resolve_identity("cto").unwrap(), "cro");

    let done = mailspace.move_task("cro", &handle, "done", None).unwrap();
    assert_eq!(done, handle);
    assert!(mailspace.list("cro", "tasks").unwrap().is_empty());
    assert_eq!(mailspace.list("cro", "done").unwrap().len(), 1);

    let status = mailspace.status().unwrap();
    let cro_status = status
        .identities
        .iter()
        .find(|identity| identity.identity == "cro")
        .unwrap();
    assert_eq!(cro_status.done, 1);
}

#[test]
fn rename_identity_rejects_unknown_or_colliding_names() {
    let tmp = tempfile::tempdir().unwrap();
    let mut mailspace = Mailspace::init(Some(tmp.path())).unwrap();
    mailspace.add_identity("ceo").unwrap();
    mailspace.add_identity("cto").unwrap();

    let err = mailspace.rename_identity("missing", "vp").unwrap_err();
    assert!(err.to_string().contains("unknown local identity"));

    let err = mailspace.rename_identity("ceo", "cto").unwrap_err();
    assert!(err.to_string().contains("already exists"));
}

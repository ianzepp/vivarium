use super::*;
use std::time::Instant;

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

#[test]
fn explicit_thread_skips_unrelated_messages() {
    let (_tmp, mailspace, handle) = fixture_with_noise(80);
    let messages = mailspace.thread(&handle, false, 50, 50).unwrap();
    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].kind.as_deref(), Some("need"));
    assert_eq!(messages[0].subject, "target need");
    assert!(messages[0].body.contains("need body"));
}

#[test]
fn explicit_thread_stays_cheap_with_many_unrelated_messages() {
    let (_tmp, mailspace, handle) = fixture_with_noise(200);
    // Warm SQLite/page cache so the sample measures path cost, not cold open.
    let _ = mailspace.thread(&handle, false, 50, 50).unwrap();
    let optimized = sample_ms(|| {
        let messages = mailspace.thread(&handle, false, 50, 50).unwrap();
        assert_eq!(messages.len(), 1);
    });
    // Full-candidate scan still used by inference; prove it is the old bottleneck.
    let full_scan = sample_ms(|| {
        let messages = mailspace.thread(&handle, true, 50, 50).unwrap();
        assert_eq!(messages.len(), 1);
    });
    assert!(
        optimized < full_scan / 3.0 || optimized < 40.0,
        "optimized={optimized:.1}ms full_scan={full_scan:.1}ms"
    );
    assert!(
        optimized < full_scan,
        "optimized={optimized:.1}ms should beat full_scan={full_scan:.1}ms"
    );
}

fn fixture_with_noise(noise: usize) -> (tempfile::TempDir, Mailspace, String) {
    let tmp = tempfile::tempdir().unwrap();
    let mut mailspace = Mailspace::init(Some(tmp.path())).unwrap();
    mailspace.add_identity("alice").unwrap();
    mailspace.add_identity("bob").unwrap();
    for i in 0..noise {
        mailspace
            .send(SendRequest {
                from: "alice".into(),
                to: vec!["bob".into()],
                cc: Vec::new(),
                bcc: Vec::new(),
                subject: format!("noise {i}"),
                body: format!("noise body {i}"),
                role: "inbox".into(),
                kind: None,
                reply_to: None,
            })
            .unwrap();
    }
    let sent = mailspace
        .send(SendRequest {
            from: "alice".into(),
            to: vec!["bob".into()],
            cc: Vec::new(),
            bcc: Vec::new(),
            subject: "target need".into(),
            body: "need body to show".into(),
            role: "needs".into(),
            kind: Some("need".into()),
            reply_to: None,
        })
        .unwrap();
    (tmp, mailspace, sent.delivered[0].handle.clone())
}

fn sample_ms(mut op: impl FnMut()) -> f64 {
    let mut total = 0.0;
    for _ in 0..3 {
        let start = Instant::now();
        op();
        total += start.elapsed().as_secs_f64() * 1000.0;
    }
    total / 3.0
}

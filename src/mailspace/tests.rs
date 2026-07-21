use super::*;
use crate::storage::MessageIngestRequest;
use std::collections::BTreeSet;
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

    assert!(err.to_string().contains("unknown local role"));
}

#[test]
fn absorb_mail_marks_message_read() {
    // Absorb means "read, processed, loaded into context", so it must mark the
    // message read. Otherwise absorbed mail inflates `inbox_unread`, which
    // boards and sensors (and Minds) read on as a neglect signal.
    let tmp = tempfile::tempdir().unwrap();
    let mut mailspace = Mailspace::init(Some(tmp.path())).unwrap();
    mailspace.add_identity("ceo").unwrap();
    mailspace.add_identity("cto").unwrap();
    let unread = |ms: &Mailspace| {
        ms.status()
            .unwrap()
            .identities
            .iter()
            .find(|identity| identity.identity == "cto")
            .unwrap()
            .inbox_unread
    };

    let sent = mailspace
        .send(SendRequest {
            from: "ceo".into(),
            to: vec!["cto".into()],
            cc: Vec::new(),
            bcc: Vec::new(),
            subject: "absorb me".into(),
            body: "read, processed, loaded into context".into(),
            role: "inbox".into(),
            kind: None,
            reply_to: None,
        })
        .unwrap();
    let handle = sent.delivered[0].handle.clone();

    assert_eq!(unread(&mailspace), 1, "delivered inbox mail starts unread");
    let absorbed = mailspace.absorb_mail("cto", &handle, None).unwrap();
    assert_eq!(absorbed, handle);
    assert_eq!(
        unread(&mailspace),
        0,
        "absorbed mail must no longer count as unread"
    );
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
    assert!(err.to_string().contains("unknown local role"));

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

#[test]
fn deliver_raw_rejects_unresolved_recipients() {
    let tmp = tempfile::tempdir().unwrap();
    let mut mailspace = Mailspace::init(Some(tmp.path())).unwrap();
    mailspace.add_identity("ceo").unwrap();

    // External address — resolve_identity rejects before delivery.
    let eml = b"From: stranger@example.com\r\nTo: nobody@example.com\r\nSubject: hi\r\n\r\nbody";
    let err = mailspace.deliver_raw(eml, "inbox").unwrap_err();
    assert!(err.to_string().contains("not allowed"));
}

#[test]
fn deliver_raw_multi_recipient_is_atomic_on_success() {
    let tmp = tempfile::tempdir().unwrap();
    let mut mailspace = Mailspace::init(Some(tmp.path())).unwrap();
    mailspace.add_identity("alice").unwrap();
    mailspace.add_identity("bob").unwrap();
    mailspace.add_identity("carol").unwrap();

    let to = mailspace.address_for("alice");
    let cc1 = mailspace.address_for("bob");
    let cc2 = mailspace.address_for("carol");
    let eml = format!(
        "From: sender@example.com\r\n\
To: {to}\r\n\
Cc: {cc1}, {cc2}\r\n\
Subject: multi\r\n\r\nbody"
    );
    let delivered = mailspace.deliver_raw(eml.as_bytes(), "inbox").unwrap();
    let delivered_names: BTreeSet<String> = delivered.iter().map(|d| d.identity.clone()).collect();
    assert_eq!(
        delivered_names,
        ["alice", "bob", "carol"]
            .into_iter()
            .map(String::from)
            .collect()
    );

    // Every recipient has a message row and at least one delivery event.
    for identity in &["alice", "bob", "carol"] {
        let inbox = mailspace.list(identity, "inbox").unwrap();
        assert_eq!(inbox.len(), 1, "{identity} should have one inbox message");
    }

    // Verify events were logged atomically alongside message rows.
    let storage = mailspace.storage().unwrap();
    let events = storage.list_mailspace_events_after(0).unwrap();
    let delivered_events: Vec<_> = events
        .iter()
        .filter(|e| e.command == "mail deliver")
        .collect();
    assert_eq!(
        delivered_events.len(),
        3,
        "all three recipients should have delivery events"
    );
}

#[test]
fn deliver_raw_batch_rollback_leaves_no_partial_state() {
    let tmp = tempfile::tempdir().unwrap();
    let mut mailspace = Mailspace::init(Some(tmp.path())).unwrap();
    mailspace.add_identity("alice").unwrap();
    mailspace.add_identity("bob").unwrap();
    mailspace.add_identity("carol").unwrap();

    let eml = b"From: sender@example.com\r\n\
To: alice@vivarium.local\r\n\
Cc: bob@vivarium.local, carol@vivarium.local\r\n\
Subject: rollback\r\n\r\nbody";

    let recipients = ["alice", "bob", "carol"];
    let requests: Vec<_> = recipients
        .iter()
        .map(|r| MessageIngestRequest {
            account: r.to_string(),
            local_role: "inbox".into(),
            read_state: false,
            starred: false,
            message_id_hint: None,
            seed_hint: format!("raw-delivery\0{r}\0{}", eml.len()),
            remote: None,
        })
        .collect();

    let mut storage = mailspace.storage().unwrap();

    // Inject failure after 2 message rows — proving the entire batch rolls back.
    let err = storage
        .deliver_raw_batch_fail_after(
            &requests,
            eml,
            "mail deliver",
            "delivered",
            "inbox",
            "rollback",
            2,
        )
        .unwrap_err();
    assert!(err.to_string().contains("injected batch failure"));

    // No partial message rows for ANY recipient (not even the first two).
    let all_messages = storage.list_messages_by_role("inbox").unwrap();
    assert!(
        all_messages.is_empty(),
        "no message rows should survive rollback, got {}",
        all_messages.len()
    );

    // No partial delivery events for ANY recipient.
    let events = storage.list_mailspace_events_after(0).unwrap();
    assert!(
        events.is_empty(),
        "no events should survive rollback, got {}",
        events.len()
    );
}

#[test]
fn deliver_raw_batch_success_then_list_has_events() {
    let tmp = tempfile::tempdir().unwrap();
    let mut mailspace = Mailspace::init(Some(tmp.path())).unwrap();
    mailspace.add_identity("alice").unwrap();
    mailspace.add_identity("bob").unwrap();

    let eml = b"From: sender@example.com\r\n\
To: alice@vivarium.local\r\n\
Cc: bob@vivarium.local\r\n\
Subject: batch\r\n\r\nbody";

    let recipients = ["alice", "bob"];
    let requests: Vec<_> = recipients
        .iter()
        .map(|r| MessageIngestRequest {
            account: r.to_string(),
            local_role: "inbox".into(),
            read_state: false,
            starred: false,
            message_id_hint: None,
            seed_hint: format!("raw-delivery\0{r}\0{}", eml.len()),
            remote: None,
        })
        .collect();

    let mut storage = mailspace.storage().unwrap();
    let stored = storage
        .deliver_raw_batch(
            &requests,
            eml,
            "mail deliver",
            "delivered",
            "inbox",
            "batch",
        )
        .unwrap();
    assert_eq!(stored.len(), 2);

    // Messages present for both recipients.
    let messages = storage.list_messages_by_role("inbox").unwrap();
    assert_eq!(messages.len(), 2);

    // Events present for both recipients.
    let events = storage.list_mailspace_events_after(0).unwrap();
    assert_eq!(events.len(), 2);
    assert!(events.iter().all(|e| e.command == "mail deliver"));
}

#[test]
fn legacy_identity_toml_loads_as_role_with_defaults() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    Mailspace::init(Some(root)).unwrap();
    std::fs::write(
        root.join(".vivi/mailspace.toml"),
        "name = \"demo\"\n\n[[identities]]\nname = \"hand-1\"\naliases = []\n",
    )
    .unwrap();
    let mut mailspace = Mailspace::discover(Some(root)).unwrap();
    let view = mailspace.role_view("hand-1").unwrap();
    assert_eq!(view.name, "hand-1");
    assert_eq!(view.status, "active");
    assert!(view.kind.is_none());
    assert!(view.provider.is_none());
    assert!(!view.has_charter);

    mailspace
        .set_role(
            "hand-1",
            RoleUpdate {
                kind: Some(Some("hand".into())),
                provider: Some(Some("zai".into())),
                model: Some(Some("glm-5.2".into())),
                thinking: Some(Some("low".into())),
                harness: Some(Some("subagent".into())),
                ..Default::default()
            },
        )
        .unwrap();
    mailspace
        .set_charter("hand-1", "Implement packages; report to mind.\n")
        .unwrap();
    let view = mailspace.role_view("hand-1").unwrap();
    assert_eq!(view.kind.as_deref(), Some("hand"));
    assert_eq!(view.provider.as_deref(), Some("zai"));
    assert_eq!(view.model.as_deref(), Some("glm-5.2"));
    assert_eq!(view.thinking.as_deref(), Some("low"));
    assert_eq!(view.harness.as_deref(), Some("subagent"));
    assert!(view.charter.contains("report to mind"));
}

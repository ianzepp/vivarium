use super::*;

#[test]
fn enqueue_round_trips_pending_item() {
    let tmp = tempfile::tempdir().unwrap();
    let item = QueueItem::new(
        "acct".into(),
        QueuedCommand::Archive {
            handles: vec!["one".into()],
        },
    );

    enqueue(tmp.path(), &item).unwrap();
    let loaded = load(tmp.path(), &item.id).unwrap();

    assert_eq!(loaded.status, QueueStatus::Pending);
    assert_eq!(loaded.command, item.command);
}

#[test]
fn list_hides_non_pending_by_default() {
    let tmp = tempfile::tempdir().unwrap();
    let pending = QueueItem::new(
        "acct".into(),
        QueuedCommand::Archive {
            handles: vec!["one".into()],
        },
    );
    let mut executed = QueueItem::new(
        "acct".into(),
        QueuedCommand::Archive {
            handles: vec!["two".into()],
        },
    );
    executed.mark(QueueStatus::Executed, None);

    enqueue(tmp.path(), &pending).unwrap();
    enqueue(tmp.path(), &executed).unwrap();

    assert_eq!(list(tmp.path(), false).unwrap().len(), 1);
    assert_eq!(list(tmp.path(), true).unwrap().len(), 2);
}

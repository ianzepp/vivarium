use std::path::PathBuf;

use chrono::{TimeZone, Utc};

use super::*;

#[test]
fn filter_entries_applies_date_window_and_limit() {
    let window = SyncWindow::parse(Some("2026-05-01"), Some("2026-05-03")).unwrap();
    let entries = vec![
        entry("inbox-1", 2026, 5, 3),
        entry("inbox-2", 2026, 5, 2),
        entry("inbox-3", 2026, 5, 1),
        entry("inbox-4", 2026, 4, 30),
    ];

    let filtered = filter_entries(entries, window, Some(2), None, None, None);

    assert_eq!(filtered.len(), 2);
    assert_eq!(filtered[0].message_id, "inbox-2");
    assert_eq!(filtered[1].message_id, "inbox-3");
}

#[test]
fn filter_entries_applies_text_filter_before_limit() {
    let window = SyncWindow::parse(None, None).unwrap();
    let entries = vec![
        entry_with_text("inbox-1", "DoorDash", "First deal"),
        entry_with_text("inbox-2", "Other", "No match"),
        entry_with_text("inbox-3", "DoorDash", "Second deal"),
    ];

    let filtered = filter_entries(entries, window, Some(1), Some("doordash"), None, None);

    assert_eq!(filtered.len(), 1);
    assert_eq!(filtered[0].message_id, "inbox-1");
}

#[test]
fn filter_entries_applies_read_state() {
    let window = SyncWindow::parse(None, None).unwrap();
    let entries = vec![
        entry_with_read_state("read", true),
        entry_with_read_state("unread", false),
    ];

    let filtered = filter_entries(entries, window, None, None, Some(false), None);

    assert_eq!(filtered.len(), 1);
    assert_eq!(filtered[0].message_id, "unread");
}

#[test]
fn filter_entries_applies_starred_state() {
    let window = SyncWindow::parse(None, None).unwrap();
    let entries = vec![
        entry_with_starred_state("starred", true),
        entry_with_starred_state("unstarred", false),
    ];

    let filtered = filter_entries(entries, window, None, None, None, Some(true));

    assert_eq!(filtered.len(), 1);
    assert_eq!(filtered[0].message_id, "starred");
}

fn entry(message_id: &str, year: i32, month: u32, day: u32) -> MessageEntry {
    MessageEntry {
        message_id: message_id.into(),
        from: "a@example.com".into(),
        subject: "subject".into(),
        date: Utc.with_ymd_and_hms(year, month, day, 12, 0, 0).unwrap(),
        path: PathBuf::from(format!("{message_id}.eml")),
        read_state: false,
        starred: false,
    }
}

fn entry_with_text(message_id: &str, from: &str, subject: &str) -> MessageEntry {
    MessageEntry {
        message_id: message_id.into(),
        from: from.into(),
        subject: subject.into(),
        date: Utc.with_ymd_and_hms(2026, 5, 3, 12, 0, 0).unwrap(),
        path: PathBuf::from(format!("{message_id}.eml")),
        read_state: false,
        starred: false,
    }
}

fn entry_with_read_state(message_id: &str, read_state: bool) -> MessageEntry {
    MessageEntry {
        message_id: message_id.into(),
        from: "a@example.com".into(),
        subject: "subject".into(),
        date: Utc.with_ymd_and_hms(2026, 5, 3, 12, 0, 0).unwrap(),
        path: PathBuf::from(format!("{message_id}.eml")),
        read_state,
        starred: false,
    }
}

fn entry_with_starred_state(message_id: &str, starred: bool) -> MessageEntry {
    MessageEntry {
        message_id: message_id.into(),
        from: "a@example.com".into(),
        subject: "subject".into(),
        date: Utc.with_ymd_and_hms(2026, 5, 3, 12, 0, 0).unwrap(),
        path: PathBuf::from(format!("{message_id}.eml")),
        read_state: false,
        starred,
    }
}

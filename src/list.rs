use crate::message::MessageEntry;
use crate::sync::SyncWindow;

pub fn filter_entries(
    entries: Vec<MessageEntry>,
    window: SyncWindow,
    limit: Option<usize>,
) -> Vec<MessageEntry> {
    let mut entries: Vec<MessageEntry> = entries
        .into_iter()
        .filter(|entry| window.contains_datetime(entry.date))
        .collect();
    if let Some(limit) = limit {
        entries.truncate(limit);
    }
    entries
}

pub fn print_entries(folder: &str, entries: &[MessageEntry]) {
    if entries.is_empty() {
        println!("  no messages in {folder}");
    } else {
        for entry in entries {
            println!("  {entry}");
        }
    }
    println!();
}

#[cfg(test)]
mod tests {
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

        let filtered = filter_entries(entries, window, Some(2));

        assert_eq!(filtered.len(), 2);
        assert_eq!(filtered[0].message_id, "inbox-2");
        assert_eq!(filtered[1].message_id, "inbox-3");
    }

    fn entry(message_id: &str, year: i32, month: u32, day: u32) -> MessageEntry {
        MessageEntry {
            message_id: message_id.into(),
            from: "a@example.com".into(),
            subject: "subject".into(),
            date: Utc.with_ymd_and_hms(year, month, day, 12, 0, 0).unwrap(),
            path: PathBuf::from(format!("{message_id}.eml")),
        }
    }
}

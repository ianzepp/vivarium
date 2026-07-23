use super::*;

#[test]
fn parses_watch_durations() {
    assert_eq!(parse_duration("250ms").unwrap(), Duration::from_millis(250));
    assert_eq!(parse_duration("2s").unwrap(), Duration::from_secs(2));
    assert!(parse_duration("soon").is_err());
}

#[test]
fn rejects_unknown_filter_values() {
    assert!(parse_filter_set("task,wat", "kind", &["mail", "task"]).is_err());
}

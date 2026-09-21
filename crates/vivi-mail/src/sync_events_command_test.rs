use super::*;

#[test]
fn parses_interval_units() {
    assert_eq!(parse_interval("30").unwrap(), Duration::from_secs(30));
    assert_eq!(parse_interval("30s").unwrap(), Duration::from_secs(30));
    assert_eq!(parse_interval("5m").unwrap(), Duration::from_mins(5));
    assert_eq!(parse_interval("1h").unwrap(), Duration::from_hours(1));
}

#[test]
fn rejects_invalid_interval() {
    assert!(parse_interval("0s").is_err());
    assert!(parse_interval("soon").is_err());
    assert!(parse_interval("5ms").is_err());
}

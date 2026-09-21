use super::*;

#[test]
fn normalizes_message_id() {
    assert_eq!(
        normalize_message_id(" <ABC@example.COM> "),
        Some("abc@example.com".into())
    );
    assert_eq!(normalize_message_id("<>"), None);
}

#[test]
fn extracts_message_id_from_bytes() {
    let data = b"Message-ID: <ABC@example.COM>\r\nSubject: hello\r\n\r\nbody";
    assert_eq!(
        message_id_from_bytes(data),
        Some("abc@example.com".to_string())
    );
}

#[test]
fn renders_json_message() {
    let data = b"Message-ID: <ABC@example.COM>\r\nFrom: Agent <agent@example.com>\r\nTo: Me <me@example.com>\r\nSubject: hello\r\n\r\nbody";

    let json = to_json_message("inbox-1", data).unwrap();

    assert_eq!(json["handle"], "inbox-1");
    assert_eq!(json["message_id"], "abc@example.com");
    assert_eq!(json["from"], "Agent <agent@example.com>");
    assert_eq!(json["to"][0], "Me <me@example.com>");
    assert_eq!(json["subject"], "hello");
    assert_eq!(json["body"], "body");
}

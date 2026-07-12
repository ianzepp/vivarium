use super::*;

#[test]
fn frame_round_trip() {
    let request = Request::new(7, "session.list", serde_json::json!({}));
    let mut bytes = Vec::new();
    write_frame(&mut bytes, &request).unwrap();

    let decoded: Request = read_frame(&mut bytes.as_slice()).unwrap();
    assert_eq!(decoded.method, "session.list");
    assert_eq!(decoded.id, serde_json::json!(7));
}

#[test]
fn oversized_frame_is_rejected() {
    let mut bytes = (MAX_FRAME_BYTES as u32 + 1).to_be_bytes().to_vec();
    bytes.extend([0; 4]);

    let error = read_frame::<Request>(&mut bytes.as_slice()).unwrap_err();
    assert_eq!(error.kind(), std::io::ErrorKind::InvalidData);
}

#[test]
fn key_encoding_supports_text_controls_and_chords() {
    assert_eq!(encode_key("c", &[KeyModifier::Control]).unwrap(), b"\x03");
    assert_eq!(
        encode_key("up", &[KeyModifier::Shift]).unwrap(),
        b"\x1b[1;2A"
    );
    assert_eq!(encode_key("Tab", &[KeyModifier::Shift]).unwrap(), b"\x1b[Z");
    assert_eq!(encode_key("é", &[]).unwrap(), "é".as_bytes());
}

#[test]
fn key_encoding_rejects_unsupported_or_duplicate_modifiers() {
    let duplicate = encode_key("c", &[KeyModifier::Control, KeyModifier::Control]);
    assert!(duplicate.unwrap_err().contains("duplicate"));

    let unsupported = encode_key("not-a-key", &[]);
    assert!(unsupported.unwrap_err().contains("unsupported"));
}

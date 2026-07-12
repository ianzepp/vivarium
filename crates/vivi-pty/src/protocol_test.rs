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

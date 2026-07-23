use super::*;

#[test]
fn metadata_fetch_items_include_message_id_header() {
    assert!(METADATA_FETCH_ITEMS.contains("UID"));
    assert!(METADATA_FETCH_ITEMS.contains("FLAGS"));
    assert!(METADATA_FETCH_ITEMS.contains("RFC822.SIZE"));
    assert!(METADATA_FETCH_ITEMS.contains("BODY.PEEK[HEADER.FIELDS (MESSAGE-ID)]"));
}

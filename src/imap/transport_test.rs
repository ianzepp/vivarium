use super::*;

#[test]
fn builds_xoauth2_initial_response() {
    assert_eq!(
        xoauth2_initial_response("me@example.com", "token"),
        "user=me@example.com\u{1}auth=Bearer token\u{1}\u{1}"
    );
}

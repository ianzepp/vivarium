use super::*;

#[test]
fn parses_callback_code() {
    let request = "GET /callback?code=abc%20123&scope=x HTTP/1.1\r\n\r\n";
    assert_eq!(parse_callback_code(request).unwrap(), "abc 123");
}

#[test]
fn parses_callback_error() {
    let request = "GET /callback?error=access_denied HTTP/1.1\r\n\r\n";
    assert!(
        parse_callback_code(request)
            .unwrap_err()
            .to_string()
            .contains("access_denied")
    );
}

#[test]
fn encodes_mail_scope() {
    assert_eq!(
        percent_encode("https://mail.google.com/"),
        "https%3A%2F%2Fmail.google.com%2F"
    );
}

#[test]
fn authorization_url_includes_provider_scope() {
    let url = authorization_url(
        "https://accounts.google.com/o/oauth2/v2/auth",
        "my-client",
        "https://mail.google.com/",
        "http://127.0.0.1:8080/callback",
    );
    assert!(url.contains("https%3A%2F%2Fmail.google.com%2F"));
    assert!(url.contains("client_id=my-client"));
}

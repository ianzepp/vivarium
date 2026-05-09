use serde_json::Value;
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::sync::oneshot;

use super::{ProtonApiClient, ProtonSession, ProtonSessionStore, TwoFaInfo};

#[tokio::test]
async fn auth_info_posts_username_without_secret_material() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let endpoint = format!("http://{}", listener.local_addr().unwrap());
    let (tx, rx) = oneshot::channel();

    tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        let request = read_http_request(&mut stream).await;
        let _ = tx.send(request);
        let body = r#"{"Version":4,"Modulus":"m","ServerEphemeral":"s","Salt":"salt","SRPSession":"session"}"#;
        let response = format!(
            "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\n\r\n{body}",
            body.len()
        );
        stream.write_all(response.as_bytes()).await.unwrap();
    });

    let auth_info = ProtonApiClient::new(endpoint)
        .auth_info("agent@proton.me")
        .await
        .unwrap();

    assert_eq!(auth_info.version, 4);
    let request = rx.await.unwrap();
    assert!(request.starts_with("POST /auth/v4/info HTTP/1.1"));
    assert!(request.contains("x-pm-appversion: "));
    let body = request.split("\r\n\r\n").nth(1).unwrap();
    let body: Value = serde_json::from_str(body).unwrap();
    assert_eq!(body["Username"], "agent@proton.me");
    assert!(body.get("Password").is_none());
    assert!(body.get("ClientProof").is_none());
}

#[tokio::test]
async fn refresh_posts_session_tokens_and_returns_refreshed_session() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let endpoint = format!("http://{}", listener.local_addr().unwrap());
    let (tx, rx) = oneshot::channel();

    tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        let request = read_http_request(&mut stream).await;
        let _ = tx.send(request);
        let body = r#"{"UID":"uid-2","UserID":"user-2","AccessToken":"access-2","RefreshToken":"refresh-2","Scope":"full mail","PasswordMode":1}"#;
        let response = format!(
            "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\n\r\n{body}",
            body.len()
        );
        stream.write_all(response.as_bytes()).await.unwrap();
    });

    let refreshed = ProtonApiClient::new(endpoint)
        .refresh(&fake_session())
        .await
        .unwrap();

    assert_eq!(refreshed.uid, "uid-2");
    assert_eq!(refreshed.access_token, "access-2");
    assert_eq!(refreshed.refresh_token, "refresh-2");
    assert_eq!(refreshed.user_id, "user-2");
    let request = rx.await.unwrap();
    assert!(request.starts_with("POST /auth/v4/refresh HTTP/1.1"));
    assert!(request.contains("authorization: Bearer access-1"));
    assert!(request.contains("x-pm-uid: uid-1"));
    let body = request.split("\r\n\r\n").nth(1).unwrap();
    let body: Value = serde_json::from_str(body).unwrap();
    assert_eq!(body["UID"], "uid-1");
    assert_eq!(body["AccessToken"], "access-1");
    assert_eq!(body["RefreshToken"], "refresh-1");
    assert_eq!(body["GrantType"], "refresh_token");
}

#[tokio::test]
async fn refresh_preserves_missing_diagnostic_metadata() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let endpoint = format!("http://{}", listener.local_addr().unwrap());

    tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        let _ = read_http_request(&mut stream).await;
        let body = r#"{"UID":"uid-2","AccessToken":"access-2","RefreshToken":"refresh-2"}"#;
        let response = format!(
            "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\n\r\n{body}",
            body.len()
        );
        stream.write_all(response.as_bytes()).await.unwrap();
    });

    let refreshed = ProtonApiClient::new(endpoint)
        .refresh(&fake_session())
        .await
        .unwrap();

    assert_eq!(refreshed.uid, "uid-2");
    assert_eq!(refreshed.user_id, "user-1");
    assert_eq!(refreshed.scope, "full mail");
    assert_eq!(refreshed.password_mode, 1);
}

#[tokio::test]
async fn identity_sends_auth_headers_and_redacts_key_material() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let endpoint = format!("http://{}", listener.local_addr().unwrap());
    let (tx, rx) = oneshot::channel();

    tokio::spawn(async move {
        let mut requests = Vec::new();
        let (mut stream, _) = listener.accept().await.unwrap();
        requests.push(read_http_request(&mut stream).await);
        write_json_response(&mut stream, user_body()).await;

        let (mut stream, _) = listener.accept().await.unwrap();
        requests.push(read_http_request(&mut stream).await);
        write_json_response(&mut stream, addresses_body()).await;
        let _ = tx.send(requests);
    });

    let (_session, identity) = ProtonApiClient::new(endpoint)
        .identity(&fake_session())
        .await
        .unwrap();

    assert_eq!(identity.user.email, "agent@proton.test");
    assert_eq!(identity.user.keys.private_key_present_count, 1);
    assert_eq!(identity.addresses.len(), 1);
    assert_eq!(identity.key_state.address_key_count, 1);
    assert_eq!(identity.key_state.locked_key_hint_count, 1);
    let json = serde_json::to_string(&identity).unwrap();
    assert!(!json.contains("USER_PRIVATE_KEY"));
    assert!(!json.contains("ADDRESS_PRIVATE_KEY"));
    assert!(!json.contains("ADDRESS_PUBLIC_KEY"));
    assert!(!json.contains("activation-secret"));
    assert!(!json.contains("token-secret"));

    let requests = rx.await.unwrap();
    assert!(requests[0].starts_with("GET /core/v4/users HTTP/1.1"));
    assert!(requests[0].contains("authorization: Bearer access-1"));
    assert!(requests[0].contains("x-pm-uid: uid-1"));
    assert!(requests[1].starts_with("GET /core/v4/addresses HTTP/1.1"));
    assert!(requests[1].contains("authorization: Bearer access-1"));
    assert!(requests[1].contains("x-pm-uid: uid-1"));
}

#[tokio::test]
async fn identity_refreshes_once_after_unauthorized() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let endpoint = format!("http://{}", listener.local_addr().unwrap());
    let (tx, rx) = oneshot::channel();

    tokio::spawn(async move {
        let mut requests = Vec::new();

        let (mut stream, _) = listener.accept().await.unwrap();
        requests.push(read_http_request(&mut stream).await);
        write_status_response(&mut stream, "401 Unauthorized", r#"{"Error":"expired"}"#).await;

        let (mut stream, _) = listener.accept().await.unwrap();
        requests.push(read_http_request(&mut stream).await);
        write_json_response(
            &mut stream,
            r#"{"UID":"uid-2","AccessToken":"access-2","RefreshToken":"refresh-2","Scope":"full mail","PasswordMode":1}"#,
        )
        .await;

        let (mut stream, _) = listener.accept().await.unwrap();
        requests.push(read_http_request(&mut stream).await);
        write_json_response(&mut stream, user_body()).await;

        let (mut stream, _) = listener.accept().await.unwrap();
        requests.push(read_http_request(&mut stream).await);
        write_json_response(&mut stream, addresses_body()).await;

        let _ = tx.send(requests);
    });

    let (session, identity) = ProtonApiClient::new(endpoint)
        .identity(&fake_session())
        .await
        .unwrap();

    assert_eq!(session.uid, "uid-2");
    assert_eq!(session.access_token, "access-2");
    assert_eq!(identity.user.email, "agent@proton.test");
    let requests = rx.await.unwrap();
    assert!(requests[0].starts_with("GET /core/v4/users HTTP/1.1"));
    assert!(requests[1].starts_with("POST /auth/v4/refresh HTTP/1.1"));
    assert!(requests[2].starts_with("GET /core/v4/users HTTP/1.1"));
    assert!(requests[2].contains("authorization: Bearer access-2"));
    assert!(requests[2].contains("x-pm-uid: uid-2"));
    assert!(requests[3].starts_with("GET /core/v4/addresses HTTP/1.1"));
    assert!(requests[3].contains("authorization: Bearer access-2"));
}

#[tokio::test]
async fn list_messages_sends_auth_headers_and_parses_metadata() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let endpoint = format!("http://{}", listener.local_addr().unwrap());
    let (tx, rx) = oneshot::channel();

    tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        let request = read_http_request(&mut stream).await;
        let _ = tx.send(request);
        write_json_response(
            &mut stream,
            r#"{"Total":1,"Messages":[{"ID":"proton-id","ConversationID":"conversation-id","ExternalID":"external@example.com","Subject":"subject","Time":1778205000,"Size":42,"Flags":4,"Unread":1,"NumAttachments":2,"LabelIDs":["0","5"],"Sender":{"Name":"Sender","Address":"sender@example.com"},"ToList":[{"Name":"","Address":"to@example.com"}]}]}"#,
        )
        .await;
    });

    let (_session, messages, total) = ProtonApiClient::new(endpoint)
        .list_messages(&fake_session(), 2, 25)
        .await
        .unwrap();

    assert_eq!(total, 1);
    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].id, "proton-id");
    assert_eq!(messages[0].sender.address, "sender@example.com");
    assert_eq!(messages[0].to[0].address, "to@example.com");
    let request = rx.await.unwrap();
    assert!(request.starts_with("GET /mail/v4/messages?Page=2&PageSize=25 HTTP/1.1"));
    assert!(request.contains("authorization: Bearer access-1"));
    assert!(request.contains("x-pm-uid: uid-1"));
}

#[tokio::test]
async fn key_material_fetches_private_keys_and_salts_without_json_summary() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let endpoint = format!("http://{}", listener.local_addr().unwrap());
    let (tx, rx) = oneshot::channel();

    tokio::spawn(async move {
        let mut requests = Vec::new();
        let (mut stream, _) = listener.accept().await.unwrap();
        requests.push(read_http_request(&mut stream).await);
        write_json_response(&mut stream, user_body()).await;

        let (mut stream, _) = listener.accept().await.unwrap();
        requests.push(read_http_request(&mut stream).await);
        write_json_response(&mut stream, addresses_body()).await;

        let (mut stream, _) = listener.accept().await.unwrap();
        requests.push(read_http_request(&mut stream).await);
        write_json_response(&mut stream, key_salts_body()).await;
        let _ = tx.send(requests);
    });

    let (_session, material) = ProtonApiClient::new(endpoint)
        .key_material(&fake_session())
        .await
        .unwrap();

    assert_eq!(material.user_keys[0].id, "user-key-1");
    assert_eq!(material.user_keys[0].private_key, "USER_PRIVATE_KEY");
    assert_eq!(material.address_keys[0].private_key, "ADDRESS_PRIVATE_KEY");
    assert_eq!(
        material.address_keys[0].token.as_deref(),
        Some("token-secret")
    );
    assert_eq!(
        material.address_keys[0].signature.as_deref(),
        Some("signature-secret")
    );
    assert_eq!(material.key_salts[0].key_id, "user-key-1");
    assert_eq!(material.key_salts[0].key_salt, "salt-b64");
    assert_eq!(material.key_salts.len(), 1);
    let requests = rx.await.unwrap();
    assert!(requests[0].starts_with("GET /core/v4/users HTTP/1.1"));
    assert!(requests[1].starts_with("GET /core/v4/addresses HTTP/1.1"));
    assert!(requests[2].starts_with("GET /core/v4/keys/salts HTTP/1.1"));
}

#[tokio::test]
async fn fetch_message_parses_header_and_armored_body() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let endpoint = format!("http://{}", listener.local_addr().unwrap());
    let (tx, rx) = oneshot::channel();

    tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        let request = read_http_request(&mut stream).await;
        let _ = tx.send(request);
        write_json_response(
            &mut stream,
            r#"{"Message":{"ID":"proton-id","Subject":"subject","Header":"Subject: subject\r\n\r\n","Body":"-----BEGIN PGP MESSAGE-----","MIMEType":"text/plain"}}"#,
        )
        .await;
    });

    let (_session, message) = ProtonApiClient::new(endpoint)
        .fetch_message(&fake_session(), "proton-id")
        .await
        .unwrap();

    assert_eq!(message.metadata.id, "proton-id");
    assert_eq!(message.header, "Subject: subject\r\n\r\n");
    assert_eq!(message.body, "-----BEGIN PGP MESSAGE-----");
    let request = rx.await.unwrap();
    assert!(request.starts_with("GET /mail/v4/messages/proton-id HTTP/1.1"));
    assert!(request.contains("authorization: Bearer access-1"));
}

#[test]
fn session_store_round_trips_with_private_permissions() {
    let tmp = tempfile::tempdir().unwrap();
    let store = ProtonSessionStore::new(tmp.path());
    store.save(&fake_session()).unwrap();

    let loaded = store.load().unwrap();
    assert_eq!(loaded.uid, "uid-1");
    assert_eq!(loaded.refresh_token, "refresh-1");
    assert!(store.path().ends_with(".vivarium/proton-session.json"));

    #[cfg(unix)]
    {
        let file_mode = std::fs::metadata(store.path())
            .unwrap()
            .permissions()
            .mode()
            & 0o777;
        let dir_mode = std::fs::metadata(store.path().parent().unwrap())
            .unwrap()
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(file_mode, 0o600);
        assert_eq!(dir_mode, 0o700);
    }
}

fn fake_session() -> ProtonSession {
    ProtonSession {
        uid: "uid-1".into(),
        access_token: "access-1".into(),
        refresh_token: "refresh-1".into(),
        app_version: "web-mail@test".into(),
        user_id: "user-1".into(),
        scope: "full mail".into(),
        password_mode: 1,
        two_fa: TwoFaInfo::default(),
        updated_at: "2026-05-09T00:00:00Z".into(),
    }
}

async fn read_http_request(stream: &mut tokio::net::TcpStream) -> String {
    let mut buffer = vec![0; 8192];
    let n = stream.read(&mut buffer).await.unwrap();
    String::from_utf8_lossy(&buffer[..n]).to_string()
}

async fn write_json_response(stream: &mut tokio::net::TcpStream, body: &str) {
    write_status_response(stream, "200 OK", body).await;
}

async fn write_status_response(stream: &mut tokio::net::TcpStream, status: &str, body: &str) {
    let response = format!(
        "HTTP/1.1 {status}\r\ncontent-type: application/json\r\nconnection: close\r\ncontent-length: {}\r\n\r\n{body}",
        body.len()
    );
    stream.write_all(response.as_bytes()).await.unwrap();
}

fn user_body() -> &'static str {
    r#"{"User":{"ID":"user-1","Name":"agent","Email":"agent@proton.test","DisplayName":"Agent","Private":1,"Keys":[{"ID":"user-key-1","PrivateKey":"USER_PRIVATE_KEY","PublicKey":"USER_PUBLIC_KEY","Fingerprint":"fp-user"}]}}"#
}

fn addresses_body() -> &'static str {
    r#"{"Addresses":[{"ID":"address-1","Email":"agent@proton.test","Status":1,"Receive":1,"Send":1,"HasKeys":1,"Keys":[{"ID":"address-key-1","Active":1,"Primary":1,"PrivateKey":"ADDRESS_PRIVATE_KEY","PublicKey":"ADDRESS_PUBLIC_KEY","Fingerprint":"fp-address","Token":"token-secret","Signature":"signature-secret","Activation":"activation-secret"}]}]}"#
}

fn key_salts_body() -> &'static str {
    r#"{"KeySalts":[{"ID":"user-key-1","KeySalt":"salt-b64"},{"ID":"old-key","KeySalt":null},{"ID":null,"KeySalt":"ignored"}]}"#
}

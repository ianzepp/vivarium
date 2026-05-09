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
    assert!(request.contains("x-pm-appversion: web-mail@"));
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

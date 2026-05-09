use serde_json::Value;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::sync::oneshot;

use super::{
    CreateDraftReq, DraftTemplate, MessagePackage, ProtonAddress, ProtonApiClient, ProtonSession,
    SendDraftReq, TwoFaInfo,
};

#[tokio::test]
async fn create_draft_posts_template_and_parses_draft_message() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let endpoint = format!("http://{}", listener.local_addr().unwrap());
    let (tx, rx) = oneshot::channel();

    tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        let request = read_http_request(&mut stream).await;
        let _ = tx.send(request);
        write_json_response(
            &mut stream,
            r#"{"Message":{"ID":"draft-id","Subject":"draft subject"}}"#,
        )
        .await;
    });

    let (_session, draft) = ProtonApiClient::new(endpoint)
        .create_draft(&fake_session(), &draft_request())
        .await
        .unwrap();

    assert_eq!(draft.id, "draft-id");
    let request = rx.await.unwrap();
    assert!(request.starts_with("POST /mail/v4/messages HTTP/1.1"));
    assert!(request.contains("authorization: Bearer access-1"));
    assert!(request.contains("x-pm-uid: uid-1"));
    let body = request.split("\r\n\r\n").nth(1).unwrap();
    let body: Value = serde_json::from_str(body).unwrap();
    assert_eq!(body["Message"]["Subject"], "draft subject");
    assert_eq!(body["Message"]["Sender"]["Address"], "sender@example.com");
    assert_eq!(body["Message"]["ToList"][0]["Address"], "to@example.com");
    assert_eq!(body["Message"]["Body"], "clear body");
    assert_eq!(body["Message"]["MIMEType"], "text/plain");
}

#[tokio::test]
async fn send_draft_posts_packages_and_parses_sent_message() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let endpoint = format!("http://{}", listener.local_addr().unwrap());
    let (tx, rx) = oneshot::channel();

    tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        let request = read_http_request(&mut stream).await;
        let _ = tx.send(request);
        write_json_response(
            &mut stream,
            r#"{"Sent":{"ID":"sent-id","Subject":"sent subject"}}"#,
        )
        .await;
    });

    let (_session, sent) = ProtonApiClient::new(endpoint)
        .send_draft(&fake_session(), "draft-id", &send_request())
        .await
        .unwrap();

    assert_eq!(sent.id, "sent-id");
    let request = rx.await.unwrap();
    assert!(request.starts_with("POST /mail/v4/messages/draft-id HTTP/1.1"));
    assert!(request.contains("authorization: Bearer access-1"));
    let body = request.split("\r\n\r\n").nth(1).unwrap();
    let body: Value = serde_json::from_str(body).unwrap();
    assert_eq!(body["Packages"][0]["MIMEType"], "text/plain");
    assert_eq!(body["Packages"][0]["Body"], "encrypted-package");
}

async fn read_http_request(stream: &mut tokio::net::TcpStream) -> String {
    let mut buffer = vec![0; 8192];
    let n = stream.read(&mut buffer).await.unwrap();
    String::from_utf8_lossy(&buffer[..n]).to_string()
}

async fn write_json_response(stream: &mut tokio::net::TcpStream, body: &str) {
    let response = format!(
        "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\nconnection: close\r\ncontent-length: {}\r\n\r\n{body}",
        body.len()
    );
    stream.write_all(response.as_bytes()).await.unwrap();
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

fn draft_request() -> CreateDraftReq {
    CreateDraftReq {
        message: DraftTemplate {
            subject: "draft subject".into(),
            sender: ProtonAddress {
                name: "Sender".into(),
                address: "sender@example.com".into(),
            },
            to: vec![ProtonAddress {
                name: String::new(),
                address: "to@example.com".into(),
            }],
            cc: Vec::new(),
            bcc: Vec::new(),
            body: "clear body".into(),
            mime_type: "text/plain".into(),
            unread: 0,
            external_id: Some("external@example.com".into()),
        },
        attachment_key_packets: Vec::new(),
        parent_id: None,
        action: 0,
    }
}

fn send_request() -> SendDraftReq {
    SendDraftReq {
        packages: vec![MessagePackage {
            addresses: serde_json::json!({"to@example.com": {"Type": 4}}),
            mime_type: "text/plain".into(),
            package_type: 4,
            body: "encrypted-package".into(),
        }],
    }
}

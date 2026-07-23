use serde_json::Value;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::sync::oneshot;

use super::{EmbeddingProvider, OllamaEmbeddingProvider};

#[tokio::test]
async fn ollama_provider_sends_embed_request_shape() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let endpoint = format!("http://{}/api/embed", listener.local_addr().unwrap());
    let (tx, rx) = oneshot::channel();

    tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        let request = read_http_request(&mut stream).await;
        let _ = tx.send(request);
        let body = "{\"embeddings\":[[0.1,0.2]]}";
        let response = format!(
            "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\n\r\n{body}",
            body.len()
        );
        stream.write_all(response.as_bytes()).await.unwrap();
    });

    let provider =
        OllamaEmbeddingProvider::new("ollama".into(), "mail-embedding-model".into(), endpoint);
    let vectors = provider.embed(&["hello".into()]).await.unwrap();
    let request = rx.await.unwrap();

    assert!(request.starts_with("POST /api/embed HTTP/1.1"));
    let body = request.split("\r\n\r\n").nth(1).unwrap();
    let parsed: Value = serde_json::from_str(body).unwrap();
    assert_eq!(parsed["model"], "mail-embedding-model");
    assert_eq!(parsed["input"][0], "hello");
    assert_eq!(vectors, vec![vec![0.1, 0.2]]);
}

async fn read_http_request(stream: &mut tokio::net::TcpStream) -> String {
    let mut buffer = Vec::new();
    let mut chunk = [0_u8; 1024];
    loop {
        let read = stream.read(&mut chunk).await.unwrap();
        assert_ne!(read, 0);
        buffer.extend_from_slice(&chunk[..read]);
        if let Some(request_len) = complete_request_len(&buffer) {
            buffer.truncate(request_len);
            return String::from_utf8(buffer).unwrap();
        }
    }
}

fn complete_request_len(buffer: &[u8]) -> Option<usize> {
    let header_end = buffer.windows(4).position(|w| w == b"\r\n\r\n")? + 4;
    let headers = String::from_utf8_lossy(&buffer[..header_end]);
    let content_len = headers
        .lines()
        .find_map(|line| line.strip_prefix("content-length: "))
        .or_else(|| {
            headers
                .lines()
                .find_map(|line| line.strip_prefix("Content-Length: "))
        })?
        .parse::<usize>()
        .ok()?;
    let request_len = header_end + content_len;
    (buffer.len() >= request_len).then_some(request_len)
}

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::error::VivariumError;

#[async_trait]
pub(crate) trait EmbeddingProvider {
    fn provider(&self) -> &str;
    fn model(&self) -> &str;
    async fn embed(&self, inputs: &[String]) -> Result<Vec<Vec<f32>>, VivariumError>;
}

pub struct OllamaEmbeddingProvider {
    provider: String,
    model: String,
    endpoint: String,
    client: reqwest::Client,
}

impl OllamaEmbeddingProvider {
    pub fn new(provider: String, model: String, endpoint: String) -> Self {
        Self {
            provider,
            model,
            endpoint,
            client: reqwest::Client::new(),
        }
    }
}

#[async_trait]
impl EmbeddingProvider for OllamaEmbeddingProvider {
    fn provider(&self) -> &str {
        &self.provider
    }

    fn model(&self) -> &str {
        &self.model
    }

    async fn embed(&self, inputs: &[String]) -> Result<Vec<Vec<f32>>, VivariumError> {
        if inputs.is_empty() {
            return Ok(Vec::new());
        }
        let response = self
            .client
            .post(&self.endpoint)
            .json(&EmbedRequest {
                model: &self.model,
                input: inputs,
            })
            .send()
            .await
            .map_err(|e| VivariumError::Other(format!("embedding provider request failed: {e}")))?;
        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(VivariumError::Other(format!(
                "embedding provider failed with {status}: {body}"
            )));
        }
        let parsed = response
            .json::<EmbedResponse>()
            .await
            .map_err(|e| VivariumError::Other(format!("embedding provider JSON failed: {e}")))?;
        validate_vectors(inputs.len(), parsed.embeddings)
    }
}

#[derive(Serialize)]
struct EmbedRequest<'a> {
    model: &'a str,
    input: &'a [String],
}

#[derive(Deserialize)]
struct EmbedResponse {
    embeddings: Vec<Vec<f32>>,
}

fn validate_vectors(
    expected: usize,
    vectors: Vec<Vec<f32>>,
) -> Result<Vec<Vec<f32>>, VivariumError> {
    if vectors.len() != expected {
        return Err(VivariumError::Other(format!(
            "embedding provider returned {} vectors for {expected} inputs",
            vectors.len()
        )));
    }
    let Some(dimensions) = vectors.first().map(Vec::len) else {
        return Ok(vectors);
    };
    if dimensions == 0 || vectors.iter().any(|vector| vector.len() != dimensions) {
        return Err(VivariumError::Other(
            "embedding provider returned inconsistent dimensions".into(),
        ));
    }
    Ok(vectors)
}

#[cfg(test)]
mod tests {
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
            OllamaEmbeddingProvider::new("ollama".into(), "cassio-embedding".into(), endpoint);
        let vectors = provider.embed(&["hello".into()]).await.unwrap();
        let request = rx.await.unwrap();

        assert!(request.starts_with("POST /api/embed HTTP/1.1"));
        let body = request.split("\r\n\r\n").nth(1).unwrap();
        let parsed: Value = serde_json::from_str(body).unwrap();
        assert_eq!(parsed["model"], "cassio-embedding");
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
}

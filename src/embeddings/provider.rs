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

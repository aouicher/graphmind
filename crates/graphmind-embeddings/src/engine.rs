use std::fmt;

#[derive(Debug)]
pub enum EmbedError {
    NotConfigured(String),
    ApiError(String),
    ModelError(String),
}

impl fmt::Display for EmbedError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotConfigured(msg) => write!(f, "Provider not configured: {msg}"),
            Self::ApiError(msg) => write!(f, "API error: {msg}"),
            Self::ModelError(msg) => write!(f, "Model error: {msg}"),
        }
    }
}

impl std::error::Error for EmbedError {}

pub trait EmbeddingEngine: Send + Sync {
    /// Embed a document (symbol text, memory entry).
    fn embed(&self, text: &str) -> Result<Vec<f32>, EmbedError>;
    /// Embed a search query. Retrieval models embed queries and documents differently;
    /// engines that support it (Voyage `input_type`) override this.
    fn embed_query(&self, text: &str) -> Result<Vec<f32>, EmbedError> {
        self.embed(text)
    }
    fn embed_batch(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>, EmbedError>;
    fn dimensions(&self) -> usize;
    fn model_id(&self) -> &str;
    fn provider_name(&self) -> &str;
    fn is_available(&self) -> bool;
}

pub struct NoopEngine;

impl EmbeddingEngine for NoopEngine {
    fn embed(&self, _text: &str) -> Result<Vec<f32>, EmbedError> {
        Err(EmbedError::NotConfigured("embeddings disabled".into()))
    }

    fn embed_batch(&self, _texts: &[&str]) -> Result<Vec<Vec<f32>>, EmbedError> {
        Err(EmbedError::NotConfigured("embeddings disabled".into()))
    }

    fn dimensions(&self) -> usize {
        0
    }

    fn model_id(&self) -> &str {
        "noop"
    }

    fn provider_name(&self) -> &str {
        "disabled"
    }

    fn is_available(&self) -> bool {
        false
    }
}

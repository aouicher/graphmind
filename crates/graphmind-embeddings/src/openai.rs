use crate::engine::{EmbedError, EmbeddingEngine};

pub struct OpenAiEngine {
    api_key: String,
    model: String,
    base_url: String,
    dims: usize,
}

impl OpenAiEngine {
    pub fn new(api_key: &str, model: &str, base_url: Option<&str>) -> Self {
        let dims = match model {
            "text-embedding-3-small" => 1536,
            "text-embedding-3-large" => 3072,
            "text-embedding-ada-002" => 1536,
            _ => 1536,
        };
        Self {
            api_key: api_key.to_string(),
            model: model.to_string(),
            base_url: base_url
                .unwrap_or("https://api.openai.com/v1")
                .trim_end_matches('/')
                .to_string(),
            dims,
        }
    }

    fn call_api(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>, EmbedError> {
        let client = reqwest::blocking::Client::new();
        let body = serde_json::json!({
            "input": texts,
            "model": self.model,
        });

        let url = format!("{}/embeddings", self.base_url);
        let resp = client
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .json(&body)
            .send()
            .map_err(|e| EmbedError::ApiError(e.to_string()))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().unwrap_or_default();
            return Err(EmbedError::ApiError(format!("{status}: {text}")));
        }

        let json: serde_json::Value = resp
            .json()
            .map_err(|e| EmbedError::ApiError(e.to_string()))?;

        let data = json["data"]
            .as_array()
            .ok_or_else(|| EmbedError::ApiError("Missing 'data' in response".into()))?;

        let mut results = Vec::with_capacity(data.len());
        for item in data {
            let embedding = item["embedding"]
                .as_array()
                .ok_or_else(|| EmbedError::ApiError("Missing 'embedding' field".into()))?
                .iter()
                .map(|v| v.as_f64().unwrap_or(0.0) as f32)
                .collect();
            results.push(embedding);
        }

        Ok(results)
    }
}

impl EmbeddingEngine for OpenAiEngine {
    fn embed(&self, text: &str) -> Result<Vec<f32>, EmbedError> {
        let results = self.call_api(&[text])?;
        results
            .into_iter()
            .next()
            .ok_or_else(|| EmbedError::ApiError("No embedding returned".into()))
    }

    fn embed_batch(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>, EmbedError> {
        let mut all_results = Vec::new();
        for chunk in texts.chunks(2048) {
            let mut batch = self.call_api(chunk)?;
            all_results.append(&mut batch);
        }
        Ok(all_results)
    }

    fn dimensions(&self) -> usize {
        self.dims
    }

    fn model_id(&self) -> &str {
        &self.model
    }

    fn provider_name(&self) -> &str {
        "openai"
    }

    fn is_available(&self) -> bool {
        true
    }
}

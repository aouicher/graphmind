use crate::engine::{EmbedError, EmbeddingEngine};

pub struct VoyageEngine {
    api_key: String,
    model: String,
    dims: usize,
}

impl VoyageEngine {
    pub fn new(api_key: &str, model: &str) -> Self {
        let dims = match model {
            "voyage-code-3" => 1024,
            "voyage-3" => 1024,
            "voyage-3-lite" => 512,
            _ => 1024,
        };
        Self {
            api_key: api_key.to_string(),
            model: model.to_string(),
            dims,
        }
    }

    fn call_api(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>, EmbedError> {
        let client = reqwest::blocking::Client::new();
        let body = serde_json::json!({
            "input": texts,
            "model": self.model,
            "input_type": "document",
        });

        let resp = client
            .post("https://api.voyageai.com/v1/embeddings")
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("Content-Type", "application/json")
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

impl EmbeddingEngine for VoyageEngine {
    fn embed(&self, text: &str) -> Result<Vec<f32>, EmbedError> {
        let results = self.call_api(&[text])?;
        results
            .into_iter()
            .next()
            .ok_or_else(|| EmbedError::ApiError("No embedding returned".into()))
    }

    fn embed_batch(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>, EmbedError> {
        let chunks: Vec<&[&str]> = texts.chunks(1024).collect();
        let concurrency = 10;
        let mut all_results: Vec<Vec<Vec<f32>>> = vec![Vec::new(); chunks.len()];

        for window_start in (0..chunks.len()).step_by(concurrency) {
            let window_end = (window_start + concurrency).min(chunks.len());
            let mut first_error: Option<EmbedError> = None;

            std::thread::scope(|s| {
                let mut handles = Vec::new();
                for (i, chunk) in chunks[window_start..window_end].iter().enumerate() {
                    let idx = window_start + i;
                    handles.push((idx, s.spawn(move || {
                        self.call_api(chunk)
                    })));
                }
                for (i, handle) in handles {
                    match handle.join().unwrap() {
                        Ok(batch) => all_results[i] = batch,
                        Err(e) => {
                            if first_error.is_none() {
                                first_error = Some(e);
                            }
                        }
                    }
                }
            });

            if let Some(e) = first_error {
                return Err(e);
            }
        }

        Ok(all_results.into_iter().flatten().collect())
    }

    fn dimensions(&self) -> usize {
        self.dims
    }

    fn model_id(&self) -> &str {
        &self.model
    }

    fn provider_name(&self) -> &str {
        "voyage"
    }

    fn is_available(&self) -> bool {
        true
    }
}

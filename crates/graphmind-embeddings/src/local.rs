use crate::engine::{EmbedError, EmbeddingEngine};
use fastembed::{EmbeddingModel, InitOptions, TextEmbedding};

pub struct LocalEngine {
    model: TextEmbedding,
    model_id: String,
    dims: usize,
}

impl LocalEngine {
    pub fn new(model_name: Option<&str>) -> Result<Self, EmbedError> {
        let name = model_name.unwrap_or("nomic-embed-text-v1.5");
        let (model_enum, dims) = match name {
            "nomic-embed-text-v1.5" => (EmbeddingModel::NomicEmbedTextV15, 768),
            "all-MiniLM-L6-v2" => (EmbeddingModel::AllMiniLML6V2, 384),
            "bge-small-en-v1.5" => (EmbeddingModel::BGESmallENV15, 384),
            other => {
                return Err(EmbedError::ModelError(format!(
                    "Unknown local model: {other}"
                )))
            }
        };

        let cache_dir = dirs::home_dir()
            .map(|h| h.join(".graphmind").join("models"))
            .unwrap_or_else(|| std::path::PathBuf::from(".graphmind/models"));
        std::fs::create_dir_all(&cache_dir).ok();

        eprintln!("Loading local embedding model ({name})... (first run downloads ~30MB)");
        let model = TextEmbedding::try_new(
            InitOptions::new(model_enum)
                .with_cache_dir(cache_dir)
                .with_show_download_progress(true),
        )
        .map_err(|e| EmbedError::ModelError(e.to_string()))?;

        Ok(Self {
            model,
            model_id: model_name.unwrap_or("all-MiniLM-L6-v2").to_string(),
            dims,
        })
    }
}

impl EmbeddingEngine for LocalEngine {
    fn embed(&self, text: &str) -> Result<Vec<f32>, EmbedError> {
        let results = self
            .model
            .embed(vec![text], None)
            .map_err(|e| EmbedError::ModelError(e.to_string()))?;
        results
            .into_iter()
            .next()
            .ok_or_else(|| EmbedError::ModelError("No embedding returned".into()))
    }

    fn embed_batch(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>, EmbedError> {
        let mut all_results = Vec::with_capacity(texts.len());
        for chunk in texts.chunks(128) {
            let mut batch = self.model
                .embed(chunk.to_vec(), None)
                .map_err(|e| EmbedError::ModelError(e.to_string()))?;
            all_results.append(&mut batch);
        }
        Ok(all_results)
    }

    fn dimensions(&self) -> usize {
        self.dims
    }

    fn model_id(&self) -> &str {
        &self.model_id
    }

    fn provider_name(&self) -> &str {
        "local"
    }

    fn is_available(&self) -> bool {
        true
    }
}

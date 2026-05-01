use crate::engine::{EmbedError, EmbeddingEngine, NoopEngine};
use graphmind_config::config::{EmbeddingConfig, EmbeddingMode};

pub fn create_engine(config: &EmbeddingConfig) -> Result<Box<dyn EmbeddingEngine>, EmbedError> {
    match config.mode {
        EmbeddingMode::Disabled => Ok(Box::new(NoopEngine)),
        EmbeddingMode::Local => {
            #[cfg(feature = "local")]
            {
                let model = config.model.as_deref();
                Ok(Box::new(crate::local::LocalEngine::new(model)?))
            }
            #[cfg(not(feature = "local"))]
            {
                Err(EmbedError::NotConfigured(
                    "Local embeddings not available (compiled without 'local' feature)".into(),
                ))
            }
        }
        EmbeddingMode::Openai => {
            let key = config
                .api_keys
                .openai
                .as_deref()
                .ok_or_else(|| EmbedError::NotConfigured("OpenAI API key not set".into()))?;
            let model = config.model.as_deref().unwrap_or("text-embedding-3-small");
            let base_url = config.openai_base_url.as_deref();
            Ok(Box::new(crate::openai::OpenAiEngine::new(key, model, base_url)))
        }
        EmbeddingMode::Voyage => {
            let key = config
                .api_keys
                .voyage
                .as_deref()
                .ok_or_else(|| EmbedError::NotConfigured("Voyage API key not set".into()))?;
            let model = config.model.as_deref().unwrap_or("voyage-code-3");
            Ok(Box::new(crate::voyage::VoyageEngine::new(key, model)))
        }
    }
}

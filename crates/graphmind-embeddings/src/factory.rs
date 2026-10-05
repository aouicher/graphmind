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
            let timeout = request_timeout(config.request_timeout_secs, base_url);
            Ok(Box::new(crate::openai::OpenAiEngine::new(key, model, base_url).with_timeout(timeout)))
        }
        EmbeddingMode::Voyage => {
            let key = config
                .api_keys
                .voyage
                .as_deref()
                .ok_or_else(|| EmbedError::NotConfigured("Voyage API key not set".into()))?;
            let model = config.model.as_deref().unwrap_or("voyage-code-3");
            let timeout = request_timeout(config.request_timeout_secs, None);
            Ok(Box::new(crate::voyage::VoyageEngine::new(key, model).with_timeout(timeout)))
        }
    }
}

/// Explicit setting wins; otherwise 300 s for a server on this machine (local models answer a
/// 256-text batch far slower than cloud APIs, especially on older laptops) and 30 s for the cloud.
pub fn request_timeout(configured_secs: Option<u64>, base_url: Option<&str>) -> std::time::Duration {
    if let Some(secs) = configured_secs {
        return std::time::Duration::from_secs(secs.max(1));
    }
    let local = base_url
        .and_then(|u| u.split("://").nth(1))
        .map(|rest| {
            let host = rest.split(['/', ':']).next().unwrap_or("");
            let host = if rest.starts_with('[') { rest.split(']').next().unwrap_or("").trim_start_matches('[') } else { host };
            matches!(host, "localhost" | "127.0.0.1" | "::1" | "0.0.0.0") || host.ends_with(".localhost")
        })
        .unwrap_or(false);
    std::time::Duration::from_secs(if local { 300 } else { 30 })
}

#[cfg(test)]
mod timeout_tests {
    use super::request_timeout;
    use std::time::Duration;

    #[test]
    fn defaults_by_host() {
        assert_eq!(request_timeout(None, None), Duration::from_secs(30));
        assert_eq!(request_timeout(None, Some("https://ai.mongodb.com/v1")), Duration::from_secs(30));
        assert_eq!(request_timeout(None, Some("http://127.0.0.1:11434/v1")), Duration::from_secs(300));
        assert_eq!(request_timeout(None, Some("http://localhost:11434/v1")), Duration::from_secs(300));
        assert_eq!(request_timeout(None, Some("http://[::1]:11434/v1")), Duration::from_secs(300));
    }

    #[test]
    fn explicit_setting_wins() {
        assert_eq!(request_timeout(Some(90), Some("http://127.0.0.1:11434/v1")), Duration::from_secs(90));
        assert_eq!(request_timeout(Some(0), None), Duration::from_secs(1));
    }
}

pub trait EmbeddingEngine {
    fn embed(&self, text: &str) -> Option<Vec<f32>>;
    fn embed_batch(&self, texts: &[&str]) -> Vec<Option<Vec<f32>>>;
    fn is_available(&self) -> bool;
}

pub struct NoopEngine;

impl EmbeddingEngine for NoopEngine {
    fn embed(&self, _text: &str) -> Option<Vec<f32>> {
        None
    }

    fn embed_batch(&self, texts: &[&str]) -> Vec<Option<Vec<f32>>> {
        vec![None; texts.len()]
    }

    fn is_available(&self) -> bool {
        false
    }
}

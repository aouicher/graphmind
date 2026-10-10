use crate::engine::EmbeddingEngine;
use crate::store::{EmbeddingStore, NewEmbeddingRow, bytes_to_float32, float32_to_bytes};
use std::collections::HashMap;
use std::path::Path;

#[derive(Debug, Clone)]
pub struct SearchResult {
    pub symbol_name: String,
    pub symbol_kind: String,
    pub file: String,
    pub text: String,
    pub score: f64,
}

fn cosine_similarity(a: &[f32], b: &[f32]) -> f64 {
    let mut dot = 0.0_f64;
    let mut norm_a = 0.0_f64;
    let mut norm_b = 0.0_f64;
    for i in 0..a.len().min(b.len()) {
        let ai = a[i] as f64;
        let bi = b[i] as f64;
        dot += ai * bi;
        norm_a += ai * ai;
        norm_b += bi * bi;
    }
    let denom = norm_a.sqrt() * norm_b.sqrt();
    if denom == 0.0 { 0.0 } else { dot / denom }
}

fn search_single(query_vec: &[f32], rows: &[(String, String, String, String, Vec<f32>)], limit: usize) -> Vec<SearchResult> {
    let mut scored: Vec<SearchResult> = rows
        .iter()
        .map(|(name, kind, file, text, emb)| SearchResult {
            symbol_name: name.clone(),
            symbol_kind: kind.clone(),
            file: file.clone(),
            text: text.clone(),
            score: cosine_similarity(query_vec, emb),
        })
        .collect();
    scored.sort_by(|a, b| {
        b.score
            .total_cmp(&a.score)
            .then_with(|| a.file.cmp(&b.file))
            .then_with(|| a.symbol_name.cmp(&b.symbol_name))
    });
    scored.truncate(limit);
    scored
}

fn rrf_merge(rankings: &[Vec<SearchResult>], k: usize) -> Vec<SearchResult> {
    let mut scores: HashMap<String, (SearchResult, f64)> = HashMap::new();

    for ranking in rankings {
        for (i, r) in ranking.iter().enumerate() {
            let key = format!("{}:{}", r.file, r.symbol_name);
            let rrf_score = 1.0 / (k + i + 1) as f64;
            scores
                .entry(key)
                .and_modify(|(_, s)| *s += rrf_score)
                .or_insert_with(|| (r.clone(), rrf_score));
        }
    }

    let mut results: Vec<SearchResult> = scores
        .into_values()
        .map(|(mut r, s)| {
            r.score = s;
            r
        })
        .collect();
    results.sort_by(|a, b| {
        b.score
            .total_cmp(&a.score)
            .then_with(|| a.file.cmp(&b.file))
            .then_with(|| a.symbol_name.cmp(&b.symbol_name))
    });
    results
}

/// Semantic search over memory entries stored in a flat embedding DB.
/// Memory entries use `symbol_name` = entry id, `symbol_kind` = "memory", `file` = "".
pub fn semantic_search_memory(
    db_path: &Path,
    query: &str,
    embed_fn: &dyn Fn(&str) -> Option<Vec<f32>>,
    limit: usize,
) -> Vec<SearchResult> {
    semantic_search(db_path, query, embed_fn, limit, Some("memory"))
}

/// Store the vector of memory entry `id` for `text`, replacing any previous one. Embeds first: a
/// failed embed leaves the old row (stale, but search ignores rows whose text no longer matches
/// the entry) rather than none. Returns whether the row was written.
pub fn reindex_memory(db_path: &Path, engine: &dyn EmbeddingEngine, id: &str, text: &str) -> bool {
    let Ok(vector) = engine.embed(text) else { return false };
    let Ok(store) = EmbeddingStore::open(db_path) else { return false };
    let _ = store.delete_by_symbol(id);
    store
        .insert_batch(&[NewEmbeddingRow {
            symbol_name: id.to_string(),
            symbol_kind: "memory".to_string(),
            file: String::new(),
            text: text.to_string(),
            embedding: float32_to_bytes(&vector),
        }])
        .is_ok()
}

pub fn semantic_search(
    db_path: &Path,
    query: &str,
    embed_fn: &dyn Fn(&str) -> Option<Vec<f32>>,
    limit: usize,
    kind_filter: Option<&str>,
) -> Vec<SearchResult> {
    let store = match EmbeddingStore::open(db_path) {
        Ok(s) => s,
        Err(_) => return vec![],
    };

    let all = match store.all() {
        Ok(rows) => rows,
        Err(_) => return vec![],
    };

    let decoded: Vec<(String, String, String, String, Vec<f32>)> = all
        .into_iter()
        .filter(|r| kind_filter.is_none_or(|k| r.symbol_kind == k))
        .map(|r| {
            let emb = bytes_to_float32(&r.embedding);
            (r.symbol_name, r.symbol_kind, r.file, r.text, emb)
        })
        .collect();

    if decoded.is_empty() {
        return vec![];
    }

    let queries: Vec<&str> = query.split(';').map(|q| q.trim()).filter(|q| !q.is_empty()).collect();

    if queries.len() == 1 {
        let query_vec = match embed_fn(queries[0]) {
            Some(v) => v,
            None => return vec![],
        };
        return search_single(&query_vec, &decoded, limit);
    }

    let mut rankings = Vec::new();
    for q in &queries {
        if let Some(query_vec) = embed_fn(q) {
            rankings.push(search_single(&query_vec, &decoded, limit * 2));
        }
    }

    let mut results = rrf_merge(&rankings, 60);
    results.truncate(limit);
    results
}

#[cfg(test)]
mod reindex_memory_tests {
    use super::*;
    use crate::engine::EmbedError;

    /// Embeds every text as [1.0], or fails every call.
    struct Fake(bool);
    impl EmbeddingEngine for Fake {
        fn embed(&self, _: &str) -> Result<Vec<f32>, EmbedError> {
            if self.0 { Ok(vec![1.0]) } else { Err(EmbedError::ApiError("down".into())) }
        }
        fn embed_batch(&self, ts: &[&str]) -> Result<Vec<Vec<f32>>, EmbedError> {
            ts.iter().map(|t| self.embed(t)).collect()
        }
        fn dimensions(&self) -> usize { 1 }
        fn model_id(&self) -> &str { "fake" }
        fn provider_name(&self) -> &str { "test" }
        fn is_available(&self) -> bool { true }
    }

    fn memory_texts(db: &Path) -> Vec<String> {
        EmbeddingStore::open(db).unwrap().all().unwrap().into_iter().map(|r| r.text).collect()
    }

    /// A new text replaces the entry's vector; a failed embed leaves the old one in place.
    #[test]
    fn replaces_on_success_keeps_on_failure() {
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("memory.db");
        assert!(reindex_memory(&db, &Fake(true), "id-1", "old wording"));
        assert!(reindex_memory(&db, &Fake(true), "id-1", "new wording"));
        assert_eq!(memory_texts(&db), ["new wording"]);
        assert!(!reindex_memory(&db, &Fake(false), "id-1", "newer wording"));
        assert_eq!(memory_texts(&db), ["new wording"], "a failed embed must not drop the vector");
    }
}

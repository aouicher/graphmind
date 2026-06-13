use std::collections::HashMap;

pub(crate) struct FusedResult {
    pub name: String,
    pub kind: String,
    pub file: String,
    pub line_start: i64,
    pub signature: Option<String>,
    pub snippet: Option<String>,
    pub content: Option<String>,
    pub score: f64,
}

impl FusedResult {
    pub fn from_symbol(s: graphmind_db::queries::SymbolRow, score: Option<f64>) -> Self {
        let snippet = s.content.as_deref().map(|c| {
            c.lines().take(3).collect::<Vec<_>>().join("\n")
        });
        Self {
            name: s.name,
            kind: s.kind,
            file: s.file,
            line_start: s.line_start,
            signature: s.signature,
            snippet,
            content: s.content,
            score: score.unwrap_or(0.0),
        }
    }
}

pub(crate) fn fuse_fts_and_semantic(
    fts: &[graphmind_db::queries::SymbolRow],
    semantic: &[graphmind_embeddings::search::SearchResult],
    limit: usize,
) -> Vec<FusedResult> {
    let k = 60.0;

    let mut scores: HashMap<String, FusedResult> = HashMap::new();

    // FTS ranking
    for (i, s) in fts.iter().enumerate() {
        let key = format!("{}:{}:{}", s.file, s.name, s.line_start);
        let rrf_score = 1.0 / (k + i as f64 + 1.0);
        let snippet = s.content.as_deref().map(|c| c.lines().take(3).collect::<Vec<_>>().join("\n"));
        scores.entry(key).or_insert_with(|| FusedResult {
            name: s.name.clone(),
            kind: s.kind.clone(),
            file: s.file.clone(),
            line_start: s.line_start,
            signature: s.signature.clone(),
            snippet,
            content: s.content.clone(),
            score: 0.0,
        }).score += rrf_score;
    }

    // Semantic ranking
    for (i, r) in semantic.iter().enumerate() {
        let key = format!("{}:{}:0", r.file, r.symbol_name);
        let rrf_score = 1.0 / (k + i as f64 + 1.0);
        scores.entry(key).or_insert_with(|| FusedResult {
            name: r.symbol_name.clone(),
            kind: r.symbol_kind.clone(),
            file: r.file.clone(),
            line_start: 0,
            signature: None,
            snippet: Some(r.text.lines().take(3).collect::<Vec<_>>().join("\n")),
            content: None,
            score: 0.0,
        }).score += rrf_score;
    }

    let mut results: Vec<FusedResult> = scores.into_values().collect();
    // Penalize test files so production code ranks higher
    for r in &mut results {
        if r.file.contains("/tests/") || r.file.contains("/test/") || r.file.contains("_test.") || r.file.contains(".test.") {
            r.score *= 0.5;
        }
    }
    results.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
    results.truncate(limit);
    results
}

pub(crate) fn load_embed_engine() -> Option<Box<dyn graphmind_embeddings::engine::EmbeddingEngine>> {
    let config = graphmind_config::config::load_config();
    // Remote embed mode: the MCP server handles semantic search via remote_semantic_search()
    if graphmind_api_client::is_remote_embed(&config) {
        return None;
    }
    if config.embedding.mode == graphmind_config::config::EmbeddingMode::Disabled {
        return None;
    }
    graphmind_embeddings::factory::create_engine(&config.embedding).ok()
}

/// Perform semantic search against the remote server (used when remote.mode = embed|full).
pub(crate) fn remote_semantic_search(
    slug: &str,
    query: &str,
    limit: usize,
) -> Vec<graphmind_embeddings::search::SearchResult> {
    let config = graphmind_config::config::load_config();
    let client = match graphmind_api_client::ApiClient::from_config(&config) {
        Ok(c) => c,
        Err(_) => return vec![],
    };
    match client.search_embeddings(slug, query, limit) {
        Ok(results) => results
            .into_iter()
            .map(|r| graphmind_embeddings::search::SearchResult {
                symbol_name: r.symbol_name,
                symbol_kind: String::new(),
                file: r.file_path.unwrap_or_default(),
                text: String::new(),
                score: r.score as f64,
            })
            .collect(),
        Err(_) => vec![],
    }
}

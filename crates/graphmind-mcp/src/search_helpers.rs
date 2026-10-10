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

/// Reciprocal-rank fusion of FTS and semantic hits. Embeddings store no line number, so a
/// semantic hit is matched to its symbol by (file, name): it adds to the FTS hit for the same
/// symbol, or is looked up with `resolve` to get its line, signature and body. Ties are broken
/// by location so the order never depends on hash order.
pub(crate) fn fuse_fts_and_semantic(
    fts: &[graphmind_db::queries::SymbolRow],
    semantic: &[graphmind_embeddings::search::SearchResult],
    limit: usize,
    resolve: impl Fn(&str, &str) -> Option<graphmind_db::queries::SymbolRow>,
) -> Vec<FusedResult> {
    let k = 60.0;

    let mut results: Vec<FusedResult> = Vec::new();
    let mut by_symbol: HashMap<(String, String), usize> = HashMap::new();
    let mut by_location: HashMap<(String, String, i64), usize> = HashMap::new();

    // FTS ranking
    for (i, s) in fts.iter().enumerate() {
        let rrf_score = 1.0 / (k + i as f64 + 1.0);
        let loc = (s.file.clone(), s.name.clone(), s.line_start);
        if let Some(&idx) = by_location.get(&loc) {
            results[idx].score += rrf_score;
            continue;
        }
        by_location.insert(loc, results.len());
        by_symbol.entry((s.file.clone(), s.name.clone())).or_insert(results.len());
        results.push(FusedResult::from_symbol(s.clone(), Some(rrf_score)));
    }

    // Semantic ranking
    for (i, r) in semantic.iter().enumerate() {
        let rrf_score = 1.0 / (k + i as f64 + 1.0);
        let key = (r.file.clone(), r.symbol_name.clone());
        if let Some(&idx) = by_symbol.get(&key) {
            results[idx].score += rrf_score;
            continue;
        }
        by_symbol.insert(key, results.len());
        match resolve(&r.file, &r.symbol_name) {
            Some(sym) => results.push(FusedResult::from_symbol(sym, Some(rrf_score))),
            None => results.push(FusedResult {
                name: r.symbol_name.clone(),
                kind: r.symbol_kind.clone(),
                file: r.file.clone(),
                line_start: 0,
                signature: None,
                snippet: Some(r.text.lines().take(3).collect::<Vec<_>>().join("\n")),
                content: None,
                score: rrf_score,
            }),
        }
    }

    // Penalize test files so production code ranks higher
    for r in &mut results {
        if r.file.contains("/tests/") || r.file.contains("/test/") || r.file.contains("_test.") || r.file.contains(".test.") {
            r.score *= 0.5;
        }
    }
    results.sort_by(|a, b| {
        b.score
            .total_cmp(&a.score)
            .then_with(|| a.file.cmp(&b.file))
            .then_with(|| a.line_start.cmp(&b.line_start))
            .then_with(|| a.name.cmp(&b.name))
    });
    results.truncate(limit);
    results
}

pub(crate) fn load_embed_engine() -> Option<Box<dyn graphmind_embeddings::engine::EmbeddingEngine>> {
    graphmind_embeddings::factory::engine_from_config(&graphmind_config::config::load_config().embedding)
}

#[cfg(test)]
mod tests {
    use super::*;
    use graphmind_db::queries::SymbolRow;
    use graphmind_embeddings::search::SearchResult;

    fn sym(file: &str, name: &str, line: i64) -> SymbolRow {
        SymbolRow {
            id: line,
            name: name.into(),
            kind: "Function".into(),
            file: file.into(),
            line_start: line,
            line_end: line + 1,
            signature: Some(format!("fn {name}()")),
            doc: None,
            content: Some(format!("fn {name}() {{}}")),
        }
    }

    fn sem(file: &str, name: &str) -> SearchResult {
        SearchResult {
            symbol_name: name.into(),
            symbol_kind: "Function".into(),
            file: file.into(),
            text: name.into(),
            score: 0.9,
        }
    }

    #[test]
    fn semantic_hit_merges_into_matching_fts_hit() {
        let fts = vec![sym("a.rs", "alpha", 10), sym("b.rs", "beta", 20)];
        let semantic = vec![sem("b.rs", "beta"), sem("c.rs", "gamma")];
        let out = fuse_fts_and_semantic(&fts, &semantic, 10, |file, name| {
            (file == "c.rs").then(|| sym(file, name, 30))
        });
        assert_eq!(out.len(), 3, "beta must not appear twice");
        assert_eq!(out[0].name, "beta", "found by both, so ranked first");
        let gamma = out.iter().find(|r| r.name == "gamma").unwrap();
        assert_eq!(gamma.line_start, 30, "semantic-only hit gets its real line");
        assert!(gamma.signature.is_some());
    }

    #[test]
    fn equal_scores_have_a_fixed_order() {
        // Same rank in each list gives the same RRF score: order falls back to file.
        let fts = vec![sym("z.rs", "zeta", 1)];
        let semantic = vec![sem("a.rs", "alpha")];
        let out = fuse_fts_and_semantic(&fts, &semantic, 10, |_, _| None);
        assert_eq!(out[0].file, "a.rs");
        assert_eq!(out[1].file, "z.rs");
    }
}

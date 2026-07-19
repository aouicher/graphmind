use graphmind_embeddings::store::{EmbeddingStore, NewEmbeddingRow, float32_to_bytes};
use graphmind_memory::search::search as memory_search;
use graphmind_memory::store::{AddOptions, MemorySource, MemoryStore, MemoryType, default_ttl_for_type};
use serde_json::Value;

use crate::formatting::{err_text, text_content};
use crate::search_helpers::load_embed_engine;

/// Maps a caller-supplied project slug to the key memory should be stored
/// under — a repo's `repo_id` when it's inside a git repo (shared across
/// worktrees), else the raw slug. See `Registry::memory_key`.
fn memory_project(slug: Option<&str>) -> Option<String> {
    slug.map(graphmind_config::Registry::memory_key)
}

pub(crate) fn handle_memory_search(args: &Value) -> Value {
    let query = match args.get("query").and_then(|v| v.as_str()) {
        Some(q) => q,
        None => return err_text("Missing required parameter: query"),
    };
    let limit = args
        .get("limit")
        .and_then(|v| v.as_u64())
        .unwrap_or(20) as usize;
    let project_slug = memory_project(args.get("project").and_then(|v| v.as_str()));
    let project_slug = project_slug.as_deref();

    let store = MemoryStore::new(&graphmind_config::paths::memory_dir());
    let all_entries = store.list(project_slug);

    // FTS results
    let fts_results = memory_search(&all_entries, query, limit * 3);

    // Semantic results (if embedding engine configured)
    let emb_db_path = graphmind_config::paths::memory_embedding_db_path();
    let semantic_ids: Vec<String> = if emb_db_path.exists() {
        if let Some(engine) = load_embed_engine() {
            graphmind_embeddings::search::semantic_search_memory(
                &emb_db_path,
                query,
                &|text| engine.embed(text).ok(),
                limit * 3,
            )
            .into_iter()
            .map(|r| r.symbol_name) // symbol_name = entry id
            .collect()
        } else {
            vec![]
        }
    } else {
        vec![]
    };

    // RRF fusion: merge FTS and semantic by entry id
    use std::collections::HashMap;
    let k = 60.0_f64;
    let mut scores: HashMap<String, (usize, f64)> = HashMap::new(); // id -> (fts_idx, score)

    // Index FTS results by id for quick lookup
    let fts_id_order: Vec<&str> = fts_results.iter().map(|e| e.id.as_str()).collect();
    for (i, id) in fts_id_order.iter().enumerate() {
        scores.entry(id.to_string()).or_insert((i, 0.0)).1 += 1.0 / (k + i as f64 + 1.0);
    }
    for (i, id) in semantic_ids.iter().enumerate() {
        scores.entry(id.clone()).or_insert((usize::MAX, 0.0)).1 += 1.0 / (k + i as f64 + 1.0);
    }

    // Build entry map for lookup
    let entry_map: HashMap<&str, &graphmind_memory::store::MemoryEntry> =
        all_entries.iter().map(|e| (e.id.as_str(), e)).collect();

    let mut ranked: Vec<(&graphmind_memory::store::MemoryEntry, f64)> = scores
        .iter()
        .filter_map(|(id, (_, score))| entry_map.get(id.as_str()).map(|e| (*e, *score)))
        .collect();
    ranked.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    ranked.truncate(limit);

    // Fall back to pure FTS if no semantic
    let results: Vec<&graphmind_memory::store::MemoryEntry> = if semantic_ids.is_empty() {
        fts_results.iter().take(limit).collect()
    } else {
        ranked.into_iter().map(|(e, _)| e).collect()
    };

    let mut lines = vec![format!(">> {} memory results for \"{}\":\n", results.len(), query)];
    for r in &results {
        let preview: String = r.content.lines().next().unwrap_or("").chars().take(100).collect();
        let r_type = format!("{:?}", r.entry_type).to_lowercase();
        lines.push(format!("  [{r_type}] {preview}"));
        // Increment recall count so high-signal entries get auto-promoted
        store.increment_recall(&r.id, project_slug);
    }
    text_content(&lines.join("\n"))
}

pub(crate) fn handle_memory_add(args: &Value) -> Value {
    let content = match args.get("content").and_then(|v| v.as_str()) {
        Some(c) => c,
        None => return err_text("Missing required parameter: content"),
    };
    let project = memory_project(args.get("project").and_then(|v| v.as_str()));
    let global = args.get("global").and_then(|v| v.as_bool()).unwrap_or(false);
    let tags: Vec<String> = args
        .get("tags")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default();

    let entry_type_str = args.get("type").and_then(|v| v.as_str()).unwrap_or("context");
    let entry_type = match entry_type_str {
        "decision" => MemoryType::Decision,
        "pattern" => MemoryType::Pattern,
        "convention" => MemoryType::Convention,
        "bug" => MemoryType::Bug,
        "session" => MemoryType::Session,
        _ => MemoryType::Context,
    };

    let priority = args.get("priority").and_then(|v| v.as_bool()).unwrap_or(false);

    let ttl_days = default_ttl_for_type(&entry_type);
    let store = MemoryStore::new(&graphmind_config::paths::memory_dir());
    let entry = store.add(
        content,
        AddOptions {
            project,
            global,
            entry_type,
            tags,
            priority,
            ttl_days,
            confidence: 1.0,
            source: MemorySource::Manual,
        },
    );

    // Vectorize and store embedding for semantic search (best-effort)
    let emb_db_path = graphmind_config::paths::memory_embedding_db_path();
    if let Some(engine) = load_embed_engine() {
        if let Ok(emb_store) = EmbeddingStore::open(&emb_db_path) {
            if let Ok(vec) = engine.embed(content) {
                let _ = emb_store.insert_batch(&[NewEmbeddingRow {
                    symbol_name: entry.id.clone(),
                    symbol_kind: "memory".to_string(),
                    file: String::new(),
                    text: content.to_string(),
                    embedding: float32_to_bytes(&vec),
                }]);
            }
        }
    }

    let prio_str = if priority { " ★priority" } else { "" };
    text_content(&format!(
        "✓ Saved to memory (id: {}, type: {}{})",
        entry.id, entry_type_str, prio_str
    ))
}

/// gm_session_analyze — batch save multiple facts with dedup.
/// Claude passes an array of facts; the tool deduplicates against existing entries
/// and saves only new ones. Returns a summary: N saved, M skipped.
pub(crate) fn handle_session_analyze(args: &Value) -> Value {
    let facts = match args.get("facts").and_then(|v| v.as_array()) {
        Some(f) => f,
        None => return err_text("Missing required parameter: facts (array)"),
    };
    let project = memory_project(args.get("project").and_then(|v| v.as_str()));
    let global = args.get("global").and_then(|v| v.as_bool()).unwrap_or(false);

    let store = MemoryStore::new(&graphmind_config::paths::memory_dir());
    let existing = store.list(project.as_deref());

    let emb_db_path = graphmind_config::paths::memory_embedding_db_path();
    let engine = load_embed_engine();

    let mut saved = 0usize;
    let mut skipped = 0usize;
    let mut saved_ids: Vec<String> = Vec::new();

    for fact in facts {
        let content = match fact.get("content").and_then(|v| v.as_str()) {
            Some(c) if !c.trim().is_empty() => c,
            _ => { skipped += 1; continue; }
        };

        // Jaccard dedup against existing entries
        let is_duplicate = existing.iter().any(|e| {
            let tokens_a: std::collections::HashSet<&str> = e.content.split_whitespace().collect();
            let tokens_b: std::collections::HashSet<&str> = content.split_whitespace().collect();
            if tokens_a.is_empty() && tokens_b.is_empty() { return true; }
            let inter = tokens_a.intersection(&tokens_b).count();
            let union = tokens_a.union(&tokens_b).count();
            if union == 0 { return false; }
            (inter as f64 / union as f64) > 0.80
        });

        if is_duplicate { skipped += 1; continue; }

        let type_str = fact.get("type").and_then(|v| v.as_str()).unwrap_or("context");
        let entry_type = match type_str {
            "decision" => MemoryType::Decision,
            "pattern" => MemoryType::Pattern,
            "convention" => MemoryType::Convention,
            "bug" => MemoryType::Bug,
            "session" => MemoryType::Session,
            _ => MemoryType::Context,
        };
        let priority = fact.get("priority").and_then(|v| v.as_bool()).unwrap_or(false);
        let tags: Vec<String> = fact.get("tags").and_then(|v| v.as_array())
            .map(|arr| arr.iter().filter_map(|v| v.as_str().map(String::from)).collect())
            .unwrap_or_default();
        let ttl_days = default_ttl_for_type(&entry_type);

        let entry = store.add(content, AddOptions {
            project: project.clone(),
            global,
            entry_type,
            tags,
            priority,
            ttl_days,
            confidence: 1.0,
            source: MemorySource::Consolidate,
        });

        // Vectorize (best-effort)
        if let Some(ref eng) = engine {
            if let Ok(emb_store) = EmbeddingStore::open(&emb_db_path) {
                if let Ok(vec) = eng.embed(content) {
                    let _ = emb_store.insert_batch(&[NewEmbeddingRow {
                        symbol_name: entry.id.clone(),
                        symbol_kind: "memory".to_string(),
                        file: String::new(),
                        text: content.to_string(),
                        embedding: float32_to_bytes(&vec),
                    }]);
                }
            }
        }

        saved_ids.push(format!("{} [{}]", &entry.id[..8], type_str));
        saved += 1;
    }

    text_content(&format!(
        "✓ Session analyzed: {} saved, {} skipped (duplicates)\n{}",
        saved,
        skipped,
        saved_ids.iter().map(|s| format!("  + {s}")).collect::<Vec<_>>().join("\n")
    ))
}

pub(crate) fn handle_memory_list(args: &Value) -> Value {
    let project_slug = memory_project(args.get("project").and_then(|v| v.as_str()));
    let project_slug = project_slug.as_deref();
    let limit = args
        .get("limit")
        .and_then(|v| v.as_u64())
        .unwrap_or(20) as usize;

    let store = MemoryStore::new(&graphmind_config::paths::memory_dir());
    let entries = store.list(project_slug);
    let truncated: Vec<_> = entries.into_iter().take(limit).collect();
    let mut lines = vec![format!(">> {} memory entries:\n", truncated.len())];
    for e in &truncated {
        let preview: String = e.content.lines().next().unwrap_or("").chars().take(80).collect();
        let e_type = format!("{:?}", e.entry_type).to_lowercase();
        lines.push(format!("  [{e_type}] {preview} (id: {})", e.id));
    }
    text_content(&lines.join("\n"))
}

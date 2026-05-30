use graphmind_embeddings::store::{EmbeddingStore, NewEmbeddingRow, float32_to_bytes};
use graphmind_memory::search::search as memory_search;
use graphmind_memory::store::{AddOptions, MemoryStore, MemoryType};
use serde_json::Value;

use crate::formatting::{err_text, text_content};
use crate::search_helpers::load_embed_engine;

pub(crate) fn handle_memory_search(args: &Value) -> Value {
    let query = match args.get("query").and_then(|v| v.as_str()) {
        Some(q) => q,
        None => return err_text("Missing required parameter: query"),
    };
    let limit = args
        .get("limit")
        .and_then(|v| v.as_u64())
        .unwrap_or(20) as usize;
    let project_slug = args.get("project").and_then(|v| v.as_str());

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
    }
    text_content(&lines.join("\n"))
}

pub(crate) fn handle_memory_add(args: &Value) -> Value {
    let content = match args.get("content").and_then(|v| v.as_str()) {
        Some(c) => c,
        None => return err_text("Missing required parameter: content"),
    };
    let project = args.get("project").and_then(|v| v.as_str()).map(String::from);
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

    let store = MemoryStore::new(&graphmind_config::paths::memory_dir());
    let entry = store.add(
        content,
        AddOptions {
            project,
            global,
            entry_type,
            tags,
            priority,
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

pub(crate) fn handle_memory_list(args: &Value) -> Value {
    let project_slug = args.get("project").and_then(|v| v.as_str());
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

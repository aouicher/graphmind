use graphmind_memory::search::search as memory_search;
use graphmind_memory::store::{AddOptions, MemoryStore, MemoryType};
use serde_json::Value;

use crate::formatting::{err_text, text_content};

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
    let entries = store.list(project_slug);
    let results = memory_search(&entries, query, limit);
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

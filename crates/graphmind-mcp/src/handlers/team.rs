use graphmind_config::config::{Feature, load_config};
use graphmind_config::paths;
use graphmind_license::LicenseManager;
use graphmind_memory::store::MemoryStore;
use serde_json::Value;

use crate::formatting::{err_text, text_content};

pub(crate) fn handle_team_memories(args: &Value) -> Value {
    let config = load_config();
    let license = LicenseManager::from_config(&config);

    if !license.has_feature(&Feature::TeamMemories) {
        return serde_json::json!({
            "content": [{ "type": "text", "text": "{\"error\": \"Team plan required.\", \"upgrade_url\": \"https://graphmind.app/pricing\"}" }],
            "isError": true
        });
    }

    let project_filter = args.get("project").and_then(|v| v.as_str());
    let search_filter = args.get("search").and_then(|v| v.as_str());
    let limit = args.get("limit").and_then(|v| v.as_u64()).unwrap_or(20) as usize;

    let store = MemoryStore::new(&paths::memory_dir());

    let mut entries = if let Some(slug) = project_filter {
        store.list(Some(slug))
    } else {
        let mut all = store.list(None);
        for p in graphmind_config::Registry::list() {
            all.extend(store.list(Some(&p.slug)));
        }
        all.dedup_by_key(|e| e.id.clone());
        all
    };

    // Only shared memories
    entries.retain(|e| e.is_shared);

    // Optional search filter
    if let Some(q) = search_filter {
        let q_lower = q.to_lowercase();
        entries.retain(|e| e.content.to_lowercase().contains(&q_lower));
    }

    entries.sort_by(|a, b| b.updated.cmp(&a.updated));
    let entries: Vec<_> = entries.into_iter().take(limit).collect();

    if entries.is_empty() {
        return text_content("Team Memories (0)\nAucune memory partagée trouvée.");
    }

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    let mut lines = vec![format!("Team Memories ({})", entries.len())];
    for e in &entries {
        let category = format!("{:?}", e.entry_type).to_lowercase();
        let project_label = e.project.as_deref().unwrap_or("global");
        let sync_label = if e.remote_id.is_some() {
            if let Some(ts) = e.synced_at {
                let diff = now.saturating_sub(ts);
                if diff < 3600 {
                    format!("Synced: {}min ago", diff / 60)
                } else if diff < 86400 {
                    format!("Synced: {}h ago", diff / 3600)
                } else {
                    format!("Synced: {}d ago", diff / 86400)
                }
            } else {
                "Synced".to_string()
            }
        } else {
            "Local only".to_string()
        };

        let preview: String = e.content.chars().take(120).collect();
        lines.push(format!(
            "[{category}] {preview}\nProject: {project_label} | {sync_label}"
        ));
    }

    text_content(&lines.join("\n"))
}

pub(crate) fn handle_team_who_knows(args: &Value) -> Value {
    let config = load_config();
    let license = LicenseManager::from_config(&config);

    if !license.has_feature(&Feature::TeamMemories) {
        return serde_json::json!({
            "content": [{ "type": "text", "text": "{\"error\": \"Team plan required.\", \"upgrade_url\": \"https://graphmind.app/pricing\"}" }],
            "isError": true
        });
    }

    let file_filter = args.get("file").and_then(|v| v.as_str());
    let symbol_filter = args.get("symbol").and_then(|v| v.as_str());

    if file_filter.is_none() && symbol_filter.is_none() {
        return err_text("At least one of 'file' or 'symbol' is required.");
    }

    let store = MemoryStore::new(&paths::memory_dir());

    let mut all = store.list(None);
    for p in graphmind_config::Registry::list() {
        all.extend(store.list(Some(&p.slug)));
    }
    all.dedup_by_key(|e| e.id.clone());
    all.retain(|e| e.is_shared);

    let matches: Vec<_> = all
        .into_iter()
        .filter(|e| {
            let content_lower = e.content.to_lowercase();
            let file_match = file_filter
                .map(|f| content_lower.contains(&f.to_lowercase()))
                .unwrap_or(false);
            let sym_match = symbol_filter
                .map(|s| content_lower.contains(&s.to_lowercase()))
                .unwrap_or(false);
            file_match || sym_match
        })
        .collect();

    let target = file_filter.or(symbol_filter).unwrap_or("");

    if matches.is_empty() {
        return text_content(&format!("Who knows about \"{target}\" (0 memories)\nAucune memory trouvée."));
    }

    let mut lines = vec![format!("Who knows about \"{}\" ({} memories)", target, matches.len())];

    for e in &matches {
        let project_label = e.project.as_deref().unwrap_or("global");
        let sync_label = if e.remote_id.is_some() {
            "Synced from team"
        } else {
            "Local memory (not yet shared)"
        };
        let preview: String = e.content.chars().take(160).collect();
        lines.push(format!("\"{preview}\"\n→ {sync_label} | Project: {project_label}"));
    }

    text_content(&lines.join("\n"))
}

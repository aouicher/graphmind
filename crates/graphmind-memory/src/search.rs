use crate::store::MemoryEntry;

pub fn search(entries: &[MemoryEntry], query: &str, limit: usize) -> Vec<MemoryEntry> {
    let terms: Vec<String> = query
        .to_lowercase()
        .split_whitespace()
        .filter(|t| t.len() > 1)
        .map(|t| t.to_string())
        .collect();

    if terms.is_empty() {
        return entries.iter().take(limit).cloned().collect();
    }

    let mut scored: Vec<(f64, &MemoryEntry)> = entries
        .iter()
        .filter_map(|entry| {
            let mut score = 0.0f64;
            let content = entry.content.to_lowercase();
            let tags: Vec<String> = entry.tags.iter().map(|t| t.to_lowercase()).collect();
            let entry_type = serde_json::to_string(&entry.entry_type)
                .unwrap_or_default()
                .trim_matches('"')
                .to_lowercase();

            for term in &terms {
                if content.contains(term.as_str()) {
                    score += 1.0;
                    let count = content.matches(term.as_str()).count();
                    score += count as f64 * 0.1;
                }

                if tags.iter().any(|t| t == term) {
                    score += 2.0;
                }

                if entry_type == *term {
                    score += 1.5;
                }
            }

            if score > 0.0 {
                Some((score, entry))
            } else {
                None
            }
        })
        .collect();

    scored.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
    scored.into_iter().take(limit).map(|(_, e)| e.clone()).collect()
}

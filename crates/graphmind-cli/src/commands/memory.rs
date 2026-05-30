use graphmind_config::{paths, resolve_project_slug};
use colored::Colorize;
use graphmind_memory::search::search as memory_search;
use graphmind_memory::store::{AddOptions, MemorySource, MemoryStore, MemoryType, default_ttl_for_type};

fn get_store() -> MemoryStore {
    MemoryStore::new(&paths::memory_dir())
}

fn parse_memory_type(s: &str) -> MemoryType {
    match s {
        "decision" => MemoryType::Decision,
        "pattern" => MemoryType::Pattern,
        "convention" => MemoryType::Convention,
        "bug" => MemoryType::Bug,
        "session" => MemoryType::Session,
        _ => MemoryType::Context,
    }
}

pub fn add(content: &str, slug: Option<&str>, global: bool, tags: &[String], entry_type: &str, priority: bool) {
    let store = get_store();
    let mem_type = parse_memory_type(entry_type);
    let ttl_days = default_ttl_for_type(&mem_type);

    let project = if global {
        None
    } else {
        match resolve_project_slug(&[slug]) {
            Some(s) => Some(s),
            None => {
                eprintln!(
                    "{} No project specified and none could be resolved. Use --global for unscoped memory.",
                    "Error:".red().bold()
                );
                std::process::exit(1);
            }
        }
    };

    let entry = store.add(
        content,
        AddOptions {
            project,
            global,
            entry_type: mem_type,
            tags: tags.to_vec(),
            priority,
            ttl_days,
            confidence: 1.0,
            source: MemorySource::Manual,
        },
    );

    let prio_str = if priority { " \u{2605}priority" } else { "" };
    println!(
        "{} Memory added: {} ({}{})",
        "OK".green().bold(),
        entry.id.dimmed(),
        entry_type,
        prio_str
    );
}

pub fn search(query: &str, slug: Option<&str>, limit: usize) {
    let store = get_store();
    let project = resolve_project_slug(&[slug]);
    let entries = store.list(project.as_deref());
    let results = memory_search(&entries, query, limit);

    if results.is_empty() {
        println!("{} No memories found for: {}", "!".yellow(), query);
        return;
    }

    println!(
        "{} {} result(s):\n",
        ">>".cyan().bold(),
        results.len().to_string().green()
    );

    for e in &results {
        let type_str = serde_json::to_string(&e.entry_type)
            .unwrap_or_default()
            .trim_matches('"')
            .to_string();
        println!(
            "  {} [{}] {}",
            e.id.get(..8).unwrap_or(&e.id).dimmed(),
            type_str.yellow(),
            e.content
        );
        if !e.tags.is_empty() {
            println!("    tags: {}", e.tags.join(", ").dimmed());
        }
        // Increment recall count for each result returned
        store.increment_recall(&e.id, project.as_deref());
    }
}

pub fn list(slug: Option<&str>, limit: usize, priority_only: bool, run_clean: bool) {
    let store = get_store();
    let project = resolve_project_slug(&[slug]);

    if run_clean {
        let summary = consolidate_steps_abcd(&store, project.as_deref(), false);
        println!("{}", summary.dimmed());
        println!();
    }

    let entries = if priority_only {
        store.list_priority(project.as_deref())
    } else {
        store.list(project.as_deref())
    };

    if entries.is_empty() {
        if priority_only {
            println!("{}", "No priority memories found.".dimmed());
        } else {
            println!("{}", "No memories found.".dimmed());
        }
        return;
    }

    let shown = entries.iter().take(limit);
    println!(
        "{} {} {}memories (showing up to {}):\n",
        ">>".cyan().bold(),
        entries.len().to_string().green(),
        if priority_only { "priority " } else { "" },
        limit
    );

    for e in shown {
        let type_str = serde_json::to_string(&e.entry_type)
            .unwrap_or_default()
            .trim_matches('"')
            .to_string();
        let prio_marker = if e.priority { "\u{2605} " } else { "" };
        println!(
            "  {}{} [{}] {}",
            prio_marker,
            e.id.get(..8).unwrap_or(&e.id).dimmed(),
            type_str.yellow(),
            e.content
        );
    }
}

pub fn delete(id: &str, slug: Option<&str>) {
    let store = get_store();
    let project = resolve_project_slug(&[slug]);
    if store.delete(id, project.as_deref()) {
        println!("{} Memory deleted: {}", "OK".green().bold(), id.dimmed());
    } else {
        eprintln!("{} Memory not found: {}", "Error:".red().bold(), id);
        std::process::exit(1);
    }
}

/// `graphmind memory clean` — steps A+B+C+D, no LLM.
pub fn clean(slug: Option<&str>) {
    let store = get_store();
    let project = resolve_project_slug(&[slug]);
    let summary = consolidate_steps_abcd(&store, project.as_deref(), false);
    println!("{} {}", "OK".green().bold(), summary);
}

/// `graphmind memory consolidate` — full pipeline, steps A-F.
pub fn consolidate(slug: Option<&str>, dry_run: bool, transcript_path: Option<&str>) {
    let store = get_store();
    let project = resolve_project_slug(&[slug]);

    // Steps A-D
    let abcd_summary = consolidate_steps_abcd(&store, project.as_deref(), dry_run);

    // Step E — LLM extraction from transcript
    let llm_added = if let Some(transcript) = transcript_path {
        consolidate_step_e(&store, project.as_deref(), transcript, dry_run)
    } else {
        0
    };

    // Step F — print summary
    println!("{} Consolidate complete:", "OK".green().bold());
    println!("  {}", abcd_summary);
    if transcript_path.is_some() {
        println!("  LLM extraction: {} new entries added", llm_added);
    }
}

// ---------------------------------------------------------------------------
// Internal implementation
// ---------------------------------------------------------------------------

/// Steps A (expire), B (commit purge), C (semantic dedup), D (auto-promote).
/// Returns a human-readable summary string.
fn consolidate_steps_abcd(store: &MemoryStore, project: Option<&str>, dry_run: bool) -> String {
    let now = chrono::Utc::now();

    let mut file_paths = vec![store.global_path()];
    if let Some(proj) = project {
        file_paths.push(store.project_path(proj));
    }

    let mut total_expired = 0usize;
    let mut total_commit = 0usize;
    let mut total_dedup = 0usize;
    let mut total_promoted = 0usize;

    for file_path in &file_paths {
        if !file_path.exists() {
            continue;
        }
        let mut entries = store.read_jsonl(file_path);
        let before = entries.len();

        // Step A — purge expired entries
        entries.retain(|e| {
            if let Some(ref exp) = e.expires_at {
                if let Ok(exp_dt) = chrono::DateTime::parse_from_rfc3339(exp) {
                    return exp_dt.with_timezone(&chrono::Utc) > now;
                }
            }
            true
        });
        let after_a = entries.len();
        total_expired += before - after_a;

        // Step B — purge [commit] entries
        entries.retain(|e| !e.content.starts_with("[commit]"));
        let after_b = entries.len();
        total_commit += after_a - after_b;

        // Step C — content-based dedup (Jaccard > 0.85)
        let dedup_ids = find_dedup_ids(&entries);
        total_dedup += dedup_ids.len();
        if !dedup_ids.is_empty() {
            entries.retain(|e| !dedup_ids.contains(&e.id));
        }

        // Step D — auto-promote entries with recall_count >= 3
        let mut promoted_in_file = 0usize;
        for entry in &mut entries {
            if entry.recall_count >= 3 && !entry.priority {
                entry.priority = true;
                entry.updated = now.to_rfc3339();
                promoted_in_file += 1;
            }
        }
        total_promoted += promoted_in_file;

        if !dry_run {
            store.rewrite_file(file_path, &entries);
        }
    }

    format!(
        "{} expired removed, {} [commit] entries removed, {} duplicates merged, {} auto-promoted{}",
        total_expired,
        total_commit,
        total_dedup,
        total_promoted,
        if dry_run { " (dry-run)" } else { "" }
    )
}

/// Identify IDs to remove for near-duplicate entries (Jaccard similarity > 0.85).
/// Keeps the entry with higher recall_count, tie-breaking by most recent created.
fn find_dedup_ids(entries: &[graphmind_memory::store::MemoryEntry]) -> std::collections::HashSet<String> {
    let mut to_remove: std::collections::HashSet<String> = std::collections::HashSet::new();

    for i in 0..entries.len() {
        if to_remove.contains(&entries[i].id) {
            continue;
        }
        for j in (i + 1)..entries.len() {
            if to_remove.contains(&entries[j].id) {
                continue;
            }
            let sim = content_jaccard(&entries[i].content, &entries[j].content);
            if sim > 0.85 {
                let keep_i = entries[i].recall_count > entries[j].recall_count
                    || (entries[i].recall_count == entries[j].recall_count
                        && entries[i].created >= entries[j].created);
                if keep_i {
                    to_remove.insert(entries[j].id.clone());
                } else {
                    to_remove.insert(entries[i].id.clone());
                    break;
                }
            }
        }
    }
    to_remove
}

/// Token-level Jaccard similarity between two content strings.
fn content_jaccard(a: &str, b: &str) -> f64 {
    use std::collections::HashSet;
    let tokens_a: HashSet<&str> = a.split_whitespace().collect();
    let tokens_b: HashSet<&str> = b.split_whitespace().collect();
    if tokens_a.is_empty() && tokens_b.is_empty() {
        return 1.0;
    }
    let intersection = tokens_a.intersection(&tokens_b).count();
    let union_count = tokens_a.union(&tokens_b).count();
    if union_count == 0 {
        return 0.0;
    }
    intersection as f64 / union_count as f64
}

/// Step E: Call Anthropic API to extract useful facts from a transcript.
/// Returns the number of new entries added (or that would be added in dry-run).
fn consolidate_step_e(
    store: &MemoryStore,
    project: Option<&str>,
    transcript_path: &str,
    dry_run: bool,
) -> usize {
    let transcript = match std::fs::read_to_string(transcript_path) {
        Ok(t) => t,
        Err(e) => {
            eprintln!(
                "{} Could not read transcript at {}: {}",
                "Warning:".yellow().bold(),
                transcript_path,
                e
            );
            return 0;
        }
    };

    let truncated: String = transcript.chars().take(8000).collect();

    let api_key = get_anthropic_api_key();
    if api_key.is_empty() {
        eprintln!(
            "{} No Anthropic API key found. Set ANTHROPIC_API_KEY or add anthropic_api_key to ~/.graphmind/config.json",
            "Warning:".yellow().bold()
        );
        return 0;
    }

    let prompt = format!(
        "You are a memory extraction assistant for a code intelligence tool.\n\n\
Analyze this session transcript and extract ONLY facts that would be useful in a FUTURE session on this same codebase.\n\n\
EXTRACT these 6 categories (with examples):\n\n\
1. ARCHITECTURAL DECISIONS — why something was built a certain way\n\
   Example: \"RRF fusion chosen over pure semantic search because FTS5 handles exact symbol names better\"\n\
   Example: \"SQLite chosen over a dedicated vector DB to keep the tool local-first with zero infrastructure\"\n\n\
2. NEGATIVE DECISIONS — what was tried and rejected, and why\n\
   Example: \"fastembed local embeddings abandoned — quality too low for cross-language symbol matching, switched to Voyage AI\"\n\
   Example: \"DefaultHasher rejected for device fingerprint — non-deterministic across Rust versions, use SHA256\"\n\n\
3. INTER-MODULE CONTRACTS — implicit interfaces the code doesn't make obvious\n\
   Example: \"Global memory lives in global.jsonl, project memory in <slug>.jsonl — never mixed, list() merges both\"\n\
   Example: \"EmbeddingStore symbol_name field stores entry id for memory entries, not a symbol name\"\n\n\
4. CONVENTIONS — naming, patterns, file structure rules\n\
   Example: \"All MCP handlers follow handle_<tool_name>(args: &Value) -> Value signature\"\n\
   Example: \"Integration tests use OnceLock<SharedProject> to build graph once, copy per test\"\n\n\
5. NON-OBVIOUS BUGS & ROOT CAUSES — subtle failures and their fix\n\
   Example: \"cargo test --doc cannot be mixed with --lib --bins — drop --doc flag\"\n\
   Example: \"UTF-8 char-boundary panic in symbol truncation — must use char_indices not byte slicing\"\n\n\
6. CRITICAL CONSTRAINTS — environment, API keys, external dependencies\n\
   Example: \"Embedding disabled silently if no VOYAGE_API_KEY — check config.embedding.mode before assuming semantic search works\"\n\
   Example: \"graphmind init requires project to be git-tracked — post-commit hook install fails silently otherwise\"\n\n\
DO NOT extract:\n\
- Task summaries (what was done, what was built)\n\
- Facts obvious from reading the code or README\n\
- Temporary state, in-progress work, TODO items\n\
- Git commit messages\n\
- Generic programming advice not specific to this codebase\n\n\
Return a JSON array (no markdown, raw JSON only):\n\
[\n\
  {{\n\
    \"content\": \"one clear atomic fact — specific, not generic\",\n\
    \"type\": \"decision|pattern|convention|bug|context\",\n\
    \"tags\": [\"tag1\", \"tag2\"],\n\
    \"priority\": false,\n\
    \"confidence\": 0.0\n\
  }}\n\
]\n\n\
Set priority=true only for facts that MUST be known at the start of every future session (critical constraints, always-apply conventions).\n\
Only include entries with confidence >= 0.7. Return [] if nothing worth saving.\n\n\
Transcript:\n\
{truncated}"
    );

    let response_text = match call_anthropic_api(&api_key, &prompt) {
        Ok(t) => t,
        Err(e) => {
            eprintln!("{} LLM API call failed: {}", "Warning:".yellow().bold(), e);
            return 0;
        }
    };

    let extracted: Vec<serde_json::Value> = match serde_json::from_str(&response_text) {
        Ok(v) => v,
        Err(e) => {
            let preview: String = response_text.chars().take(200).collect();
            eprintln!(
                "{} Could not parse LLM response as JSON: {} — response: {}",
                "Warning:".yellow().bold(),
                e,
                preview
            );
            return 0;
        }
    };

    let existing_entries = store.list(project);
    let mut added = 0usize;

    for item in &extracted {
        let confidence = item.get("confidence").and_then(|v| v.as_f64()).unwrap_or(0.0);
        if confidence < 0.7 {
            continue;
        }

        let content = match item.get("content").and_then(|v| v.as_str()) {
            Some(c) => c,
            None => continue,
        };

        // FTS dedup: skip if very similar to an existing entry
        let is_duplicate = existing_entries
            .iter()
            .any(|e| content_jaccard(&e.content, content) > 0.80);
        if is_duplicate {
            continue;
        }

        let type_str = item.get("type").and_then(|v| v.as_str()).unwrap_or("context");
        let mem_type = parse_memory_type(type_str);
        let ttl = default_ttl_for_type(&mem_type);

        let tags: Vec<String> = item
            .get("tags")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default();

        let priority = item.get("priority").and_then(|v| v.as_bool()).unwrap_or(false);

        if !dry_run {
            store.add(
                content,
                AddOptions {
                    project: project.map(String::from),
                    global: project.is_none(),
                    entry_type: mem_type,
                    tags,
                    priority,
                    ttl_days: ttl,
                    confidence: confidence as f32,
                    source: MemorySource::Consolidate,
                },
            );
        }
        added += 1;
    }

    added
}

/// Read Anthropic API key: env var takes precedence, then config.json.
fn get_anthropic_api_key() -> String {
    if let Ok(key) = std::env::var("ANTHROPIC_API_KEY") {
        if !key.is_empty() {
            return key;
        }
    }

    let config_path = graphmind_config::paths::config_path();
    if let Ok(raw) = std::fs::read_to_string(&config_path) {
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(&raw) {
            if let Some(key) = v.get("anthropic_api_key").and_then(|k| k.as_str()) {
                if !key.is_empty() {
                    return key.to_string();
                }
            }
        }
    }

    String::new()
}

/// Call Anthropic messages API and return the assistant text content.
fn call_anthropic_api(api_key: &str, prompt: &str) -> Result<String, String> {
    let body = serde_json::json!({
        "model": "claude-sonnet-4-6",
        "max_tokens": 2048,
        "messages": [
            {
                "role": "user",
                "content": prompt
            }
        ]
    });

    let client = reqwest::blocking::Client::new();
    let response = client
        .post("https://api.anthropic.com/v1/messages")
        .header("x-api-key", api_key)
        .header("anthropic-version", "2023-06-01")
        .header("content-type", "application/json")
        .json(&body)
        .timeout(std::time::Duration::from_secs(60))
        .send()
        .map_err(|e| format!("HTTP request failed: {e}"))?;

    if !response.status().is_success() {
        let status = response.status();
        let text = response.text().unwrap_or_default();
        return Err(format!("API returned {status}: {text}"));
    }

    let json: serde_json::Value = response
        .json()
        .map_err(|e| format!("Failed to parse API response: {e}"))?;

    json.get("content")
        .and_then(|c| c.as_array())
        .and_then(|arr| arr.first())
        .and_then(|first| first.get("text"))
        .and_then(|t| t.as_str())
        .map(String::from)
        .ok_or_else(|| format!("Unexpected API response shape: {json}"))
}

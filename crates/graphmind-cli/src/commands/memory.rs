use graphmind_config::{paths, resolve_project_slug, Registry};
use colored::Colorize;
use graphmind_memory::search::search as memory_search;
use graphmind_memory::store::{AddOptions, MemorySource, MemoryStore, MemoryType, default_ttl_for_type};

fn get_store() -> MemoryStore {
    MemoryStore::new(&paths::memory_dir())
}

/// Resolves `slug` to the project the memory store should key entries
/// under — a repo's `repo_id` when it's inside a git repo (shared across
/// all its worktrees), else the raw slug. See `Registry::memory_key`.
fn resolve_memory_project(slug: Option<&str>) -> Option<String> {
    resolve_project_slug(&[slug]).map(|s| Registry::memory_key(&s))
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
        match resolve_memory_project(slug) {
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
    let project = resolve_memory_project(slug);
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
        store.increment_recall(&e.id, project.as_deref());
    }
}

pub fn list(slug: Option<&str>, limit: usize, priority_only: bool, run_clean: bool) {
    let store = get_store();
    let project = resolve_memory_project(slug);

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
    let project = resolve_memory_project(slug);
    if store.delete(id, project.as_deref()) {
        println!("{} Memory deleted: {}", "OK".green().bold(), id.dimmed());
    } else {
        eprintln!("{} Memory not found: {}", "Error:".red().bold(), id);
        std::process::exit(1);
    }
}

/// `graphmind memory clean` — steps A+B+C+D, no external API.
pub fn clean(slug: Option<&str>) {
    let store = get_store();
    let project = resolve_memory_project(slug);
    let summary = consolidate_steps_abcd(&store, project.as_deref(), false);
    println!("{} {}", "OK".green().bold(), summary);
}

/// `graphmind memory consolidate` — expire, purge noise, dedup, auto-promote.
/// LLM extraction is handled by the AI agent via the stop hook — no external API needed.
pub fn consolidate(slug: Option<&str>, dry_run: bool) {
    let store = get_store();
    let project = resolve_memory_project(slug);
    let summary = consolidate_steps_abcd(&store, project.as_deref(), dry_run);
    println!("{} Consolidate complete:", "OK".green().bold());
    println!("  {}", summary);
}

/// `graphmind memory merge <target>` — folds every registered project's
/// legacy per-slug memory file that shares `target`'s repo_id into
/// `target`'s shared file (deduped by id), then removes the legacy files.
/// `target` is typically a repo_id, printed by the auto-link notice when a
/// legacy file couldn't be merged automatically.
pub fn merge(target: &str) {
    let store = get_store();
    let target_path = paths::memory_path(target);

    let sources: Vec<_> = Registry::list()
        .into_iter()
        .filter(|p| p.repo_id.as_deref() == Some(target) || p.slug == target)
        .map(|p| (p.slug.clone(), paths::memory_path(&p.slug)))
        .filter(|(_, path)| path.exists() && *path != target_path)
        .collect();

    if sources.is_empty() {
        println!("{}", "No legacy memory files found to merge.".dimmed());
        return;
    }

    let (merged_count, skipped_count, merged_slugs) = store.with_file_lock(&target_path, || {
        let mut merged: Vec<graphmind_memory::store::MemoryEntry> = if target_path.exists() {
            store.read_jsonl(&target_path)
        } else {
            Vec::new()
        };
        let mut seen: std::collections::HashSet<String> = merged.iter().map(|e| e.id.clone()).collect();

        let mut merged_count = 0usize;
        let mut skipped_count = 0usize;
        let mut merged_slugs = Vec::new();

        for (slug, path) in &sources {
            let entries = store.read_jsonl(path);
            for entry in entries {
                if seen.insert(entry.id.clone()) {
                    merged.push(entry);
                    merged_count += 1;
                } else {
                    skipped_count += 1;
                }
            }
            merged_slugs.push(slug.clone());
        }

        store.rewrite_file(&target_path, &merged);
        (merged_count, skipped_count, merged_slugs)
    });

    for (_, path) in &sources {
        std::fs::remove_file(path).ok();
    }

    println!(
        "{} Merged {} entr{} from {} legacy file(s) ({}) into '{}.jsonl'{}",
        "OK".green().bold(),
        merged_count,
        if merged_count == 1 { "y" } else { "ies" },
        sources.len(),
        merged_slugs.join(", "),
        target,
        if skipped_count > 0 {
            format!(", {skipped_count} duplicate(s) skipped")
        } else {
            String::new()
        }
    );
}

// ---------------------------------------------------------------------------
// Internal implementation
// ---------------------------------------------------------------------------

/// Steps A (expire), B (commit purge), C (semantic dedup), D (auto-promote).
fn consolidate_steps_abcd(store: &MemoryStore, project: Option<&str>, dry_run: bool) -> String {
    let now = chrono::Utc::now();

    let mut file_paths = vec![store.global_path()];
    if let Some(proj) = project {
        file_paths.push(store.project_path(proj));
    } else {
        // No specific project — consolidate all project files
        if let Ok(entries) = std::fs::read_dir(&store.memory_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().and_then(|e| e.to_str()) == Some("jsonl")
                    && path != store.global_path()
                {
                    file_paths.push(path);
                }
            }
        }
    }

    let mut total_expired = 0usize;
    let mut total_commit = 0usize;
    let mut total_dedup = 0usize;
    let mut total_promoted = 0usize;

    for file_path in &file_paths {
        if !file_path.exists() {
            continue;
        }
        let (expired, commit_purged, dedup, promoted) = store.with_file_lock(file_path, || {
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

            // Step B — purge [commit] entries
            entries.retain(|e| !e.content.starts_with("[commit]"));
            let after_b = entries.len();

            // Step C — content-based dedup (Jaccard > 0.85)
            let dedup_ids = find_dedup_ids(&entries);
            let dedup_count = dedup_ids.len();
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

            if !dry_run {
                store.rewrite_file(file_path, &entries);
            }

            (before - after_a, after_a - after_b, dedup_count, promoted_in_file)
        });
        total_expired += expired;
        total_commit += commit_purged;
        total_dedup += dedup;
        total_promoted += promoted;
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

/// Identify IDs to remove for near-duplicate entries (Jaccard > 0.85).
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

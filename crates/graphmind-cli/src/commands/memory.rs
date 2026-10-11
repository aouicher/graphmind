use graphmind_config::{paths, resolve_project_slug, Registry};
use colored::Colorize;
use graphmind_memory::consolidate::{self, Policy, Report};
use graphmind_memory::search::search as memory_search;
use graphmind_memory::store::{
    AddOptions, MemoryError, MemorySource, MemoryStore, MemoryType, MemoryUpdate, default_ttl_for_type,
};

fn get_store() -> MemoryStore {
    MemoryStore::new(&paths::memory_dir())
}

/// Resolves `slug` to the key the memory store files entries under: the
/// primary project's for a linked worktree, else the slug. See `Registry::memory_key`.
fn resolve_memory_project(slug: Option<&str>) -> Option<String> {
    resolve_project_slug(&[slug]).map(|s| Registry::memory_key(&s))
}

/// `--type` value, or exit with the list of valid types.
fn parse_memory_type(s: &str) -> MemoryType {
    s.parse::<MemoryType>().unwrap_or_else(|e| {
        eprintln!("{} {e}", "Error:".red().bold());
        std::process::exit(1);
    })
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
    )
    .unwrap_or_else(|e| {
        eprintln!("{} {e}", "Error:".red().bold());
        std::process::exit(1);
    });

    let prio_str = if priority { " \u{2605}priority" } else { "" };
    println!(
        "{} Memory added: {} ({}{})",
        "OK".green().bold(),
        entry.id.dimmed(),
        entry_type,
        prio_str
    );
}

/// `no_recall`: don't count this as a recall. Automatic searches (the prompt hook) pass it, so
/// only deliberate searches feed auto-promotion.
pub fn search(query: &str, slug: Option<&str>, limit: usize, max_chars: Option<usize>, no_recall: bool) {
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
        let tags = if e.tags.is_empty() {
            String::new()
        } else if max_chars.is_some() {
            // One line per entry: tags go on it, after the cut text.
            format!(" {}", format!("(tags: {})", e.tags.join(", ")).dimmed())
        } else {
            format!("\n    tags: {}", e.tags.join(", ").dimmed())
        };
        println!(
            "  {} [{}] {}{}",
            e.id.get(..8).unwrap_or(&e.id).dimmed(),
            e.entry_type.to_string().yellow(),
            display_content(&e.content, max_chars),
            tags
        );
    }
    if !no_recall {
        let ids: Vec<&str> = results.iter().map(|e| e.id.as_str()).collect();
        store.record_recalls(&ids, project.as_deref());
    }
}

pub fn list(slug: Option<&str>, limit: usize, priority_only: bool, run_clean: bool, max_chars: Option<usize>) {
    let store = get_store();
    let project = resolve_memory_project(slug);

    if run_clean {
        let report = run_consolidate(slug, false);
        println!("{}", summary(&report, false).dimmed());
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
        let prio_marker = if e.is_priority() { "\u{2605} " } else { "" };
        println!(
            "  {}{} [{}] {}",
            prio_marker,
            e.id.get(..8).unwrap_or(&e.id).dimmed(),
            e.entry_type.to_string().yellow(),
            display_content(&e.content, max_chars)
        );
    }
}

/// The flags of `graphmind memory update` that change the entry.
#[derive(clap::Args)]
pub struct UpdateFlags {
    /// New text
    #[arg(long)]
    content: Option<String>,
    /// Replace the tags (to remove them all, use --clear-tags)
    #[arg(long, num_args = 1..)]
    tags: Option<Vec<String>>,
    /// Remove all tags
    #[arg(long, conflicts_with = "tags")]
    clear_tags: bool,
    /// New type: decision, pattern, convention, bug, context, session
    #[arg(long)]
    r#type: Option<String>,
    /// Make it a priority entry
    #[arg(long, conflicts_with = "no_priority")]
    priority: bool,
    /// Make it a normal entry
    #[arg(long)]
    no_priority: bool,
}

impl UpdateFlags {
    /// The patch these flags describe; unset flags leave their field alone. Exits on an unknown
    /// `--type`.
    fn into_update(self) -> MemoryUpdate {
        MemoryUpdate {
            content: self.content,
            tags: if self.clear_tags { Some(Vec::new()) } else { self.tags },
            entry_type: self.r#type.as_deref().map(parse_memory_type),
            priority: match (self.priority, self.no_priority) {
                (true, _) => Some(true),
                (_, true) => Some(false),
                _ => None,
            },
        }
    }
}

/// `graphmind memory update <id>` — edit an entry in place.
pub fn update(id: &str, slug: Option<&str>, flags: UpdateFlags) {
    let store = get_store();
    let project = resolve_memory_project(slug);
    let changes = flags.into_update();
    let content_changed = changes.content.is_some();
    match store.update(id, project.as_deref(), changes) {
        Ok(e) => {
            if content_changed {
                reindex(&e.id, &e.content);
            }
            println!("{} Memory updated: {}", "OK".green().bold(), e.id.dimmed());
        }
        Err(MemoryError::NothingToChange) => {
            eprintln!(
                "{} Nothing to change: pass --content, --tags, --clear-tags, --type or --priority/--no-priority",
                "Error:".red().bold()
            );
            std::process::exit(1);
        }
        Err(e) => {
            eprintln!("{} {}: {}", "Error:".red().bold(), e, id);
            std::process::exit(1);
        }
    }
}

/// Entry text for listings. With `max_chars`, whitespace (including newlines) is collapsed so
/// each entry stays on one line, and the text is cut at `max_chars` characters with a pointer
/// to the full entry. Hooks use this to stay under Claude Code's hook output limit.
fn display_content(content: &str, max_chars: Option<usize>) -> String {
    let Some(max) = max_chars else {
        return content.to_string();
    };
    let one_line = content.split_whitespace().collect::<Vec<_>>().join(" ");
    if one_line.chars().count() <= max {
        return one_line;
    }
    let cut: String = one_line.chars().take(max).collect();
    format!("{}… (cut; `graphmind memory show <id>`)", cut.trim_end())
}

/// `graphmind memory show <id>` — one entry in full. Accepts the 8-character id prefix that
/// listings print.
pub fn show(id: &str, slug: Option<&str>) {
    let store = get_store();
    let project = resolve_memory_project(slug);
    match store.get(id, project.as_deref()) {
        Ok(e) => {
            let prio = if e.is_priority() { " \u{2605}priority" } else { "" };
            println!("{} [{}{}]", e.id.dimmed(), e.entry_type.to_string().yellow(), prio);
            println!("{}", e.content);
            if !e.tags.is_empty() {
                println!("tags: {}", e.tags.join(", ").dimmed());
            }
        }
        Err(e) => {
            eprintln!("{} {}: {}", "Error:".red().bold(), e, id);
            std::process::exit(1);
        }
    }
}

/// Re-embed an edited entry when an embedding engine is configured, so semantic search ranks it by
/// its new text (best-effort, like the MCP tools).
fn reindex(id: &str, text: &str) {
    if let Some(engine) = graphmind_embeddings::factory::engine_from_config(&graphmind_config::load_config().embedding) {
        graphmind_embeddings::search::reindex_memory(&paths::memory_embedding_db_path(), engine.as_ref(), id, text);
    }
}

pub fn delete(id: &str, slug: Option<&str>) {
    let store = get_store();
    let project = resolve_memory_project(slug);
    match store.delete_entry(id, project.as_deref()) {
        Ok(full) => println!("{} Memory deleted: {}", "OK".green().bold(), full.dimmed()),
        Err(e) => {
            eprintln!("{} {}: {}", "Error:".red().bold(), e, id);
            std::process::exit(1);
        }
    }
}

/// Runs consolidate (expire, purge noise, dedup, auto-promote) with the configured promotion
/// policy and returns what it did. `clean`, `consolidate`, `list --clean` and the daily background
/// clean (`notices::memory_auto_clean`, which prints nothing: it runs inside other commands whose
/// stdout hooks capture) differ only in how they print the report.
pub fn run_consolidate(slug: Option<&str>, dry_run: bool) -> Report {
    let store = get_store();
    let project = resolve_memory_project(slug);
    let policy = Policy {
        promote_after: graphmind_config::config::load_config().memory.auto_promote_threshold(),
        dry_run,
    };
    consolidate::consolidate(&store, project.as_deref(), &policy)
}

/// `graphmind memory clean` — steps A+B+C+D, no external API.
pub fn clean(slug: Option<&str>) {
    let report = run_consolidate(slug, false);
    println!("{} {}", "OK".green().bold(), summary(&report, false));
}

/// `graphmind memory consolidate` — expire, purge noise, dedup, auto-promote.
/// LLM extraction is left to the agent (`gm_session_analyze`, which the prompt hook suggests
/// every 10 prompts) — no external API needed.
pub fn consolidate(slug: Option<&str>, dry_run: bool) {
    let report = run_consolidate(slug, dry_run);
    println!("{} Consolidate complete:", "OK".green().bold());
    println!("  {}", summary(&report, dry_run));
}

fn summary(report: &Report, dry_run: bool) -> String {
    let mut line = format!(
        "{} expired removed, {} [commit] entries removed, {} duplicates merged, {} auto-promoted",
        report.expired, report.commits, report.duplicates, report.promoted
    );
    if report.legacy_marked > 0 {
        line.push_str(&format!(
            ", {} earlier auto-promotions marked as such (shown after pinned entries)",
            report.legacy_marked
        ));
    }
    if dry_run {
        line.push_str(" (dry-run)");
    }
    line
}

#[cfg(test)]
mod display_tests {
    use super::display_content;

    #[test]
    fn unlimited_keeps_text_as_is() {
        assert_eq!(display_content("a\nb", None), "a\nb");
    }

    #[test]
    fn limited_is_one_line_and_cut_on_char_boundary() {
        assert_eq!(display_content("a\n  b", Some(10)), "a b");
        let out = display_content("é".repeat(20).as_str(), Some(5));
        assert!(out.starts_with("ééééé…"), "{out}");
    }
}

use graphmind_config::{paths, resolve_project_slug};
use colored::Colorize;
use graphmind_memory::search::search as memory_search;
use graphmind_memory::store::{AddOptions, MemoryStore, MemoryType};

fn get_store() -> MemoryStore {
    MemoryStore::new(&paths::memory_dir())
}

pub fn add(content: &str, slug: Option<&str>, global: bool, tags: &[String], entry_type: &str) {
    let store = get_store();
    let mem_type = match entry_type {
        "decision" => MemoryType::Decision,
        "pattern" => MemoryType::Pattern,
        "convention" => MemoryType::Convention,
        "bug" => MemoryType::Bug,
        "session" => MemoryType::Session,
        _ => MemoryType::Context,
    };

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
        },
    );

    println!(
        "{} Memory added: {} ({})",
        "OK".green().bold(),
        entry.id.dimmed(),
        entry_type
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
    }
}

pub fn list(slug: Option<&str>, limit: usize) {
    let store = get_store();
    let project = resolve_project_slug(&[slug]);
    let entries = store.list(project.as_deref());

    if entries.is_empty() {
        println!("{}", "No memories found.".dimmed());
        return;
    }

    let shown = entries.iter().take(limit);
    println!(
        "{} {} total memories (showing up to {}):\n",
        ">>".cyan().bold(),
        entries.len().to_string().green(),
        limit
    );

    for e in shown {
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

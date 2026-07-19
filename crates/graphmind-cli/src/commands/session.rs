use graphmind_config::{paths, resolve_project_slug};
use colored::Colorize;
use graphmind_db::queries::GraphQueries;
use graphmind_db::schema::init_database;
use serde_json::json;
use std::fs;
use std::io::{BufRead, Write};

fn session_dir() -> std::path::PathBuf {
    let dir = paths::sessions_dir();
    fs::create_dir_all(&dir).ok();
    dir
}

fn today_log() -> std::path::PathBuf {
    let date = chrono::Local::now().format("%Y-%m-%d").to_string();
    session_dir().join(format!("{date}.jsonl"))
}

fn append_entry(slug: &str, event: &str, message: Option<&str>) {
    let mut entry = json!({
        "timestamp": chrono::Local::now().to_rfc3339(),
        "slug": slug,
        "event": event,
    });
    if let Some(msg) = message {
        entry["message"] = json!(msg);
    }
    let path = today_log();
    let mut file = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .unwrap_or_else(|e| {
            eprintln!("{} Failed to open session log: {}", "Error:".red().bold(), e);
            std::process::exit(1);
        });
    writeln!(file, "{}", entry).ok();
}

pub fn start(slug: Option<&str>) {
    let slug = match resolve_project_slug(&[slug]) {
        Some(s) => s,
        None => {
            eprintln!(
                "{} Not in a registered project. Specify a slug or register this directory.",
                "Error:".red().bold()
            );
            std::process::exit(1);
        }
    };

    append_entry(&slug, "start", None);
    println!("{} Session started for \"{}\"", "OK".green().bold(), slug);

    let db_path = paths::graph_db_path(&slug);
    if db_path.exists() {
        if let Ok(db) = init_database(&db_path.to_string_lossy()) {
            let q = GraphQueries::new(&db);
            let stats = q.stats();
            println!(
                "\n  Graph: {} symbols, {} edges, {} files",
                stats.symbols, stats.edges, stats.files
            );
        }
    }

    let mem_dir = paths::memory_dir();
    if mem_dir.exists() {
        let store = graphmind_memory::store::MemoryStore::new(&mem_dir);
        let memory_project = graphmind_config::Registry::memory_key(&slug);
        let all = store.list(Some(&memory_project));
        let recent: Vec<_> = all.into_iter().rev().take(5).collect();
        if !recent.is_empty() {
            println!("\n  Recent memories:");
            for m in &recent {
                let content_preview: String = m.content.chars().take(80).collect();
                println!("    [{}] {}", format!("{:?}", m.entry_type).to_lowercase(), content_preview);
            }
        }
    }
    println!();
}

pub fn save(message: Option<&str>, slug: Option<&str>) {
    let slug = match resolve_project_slug(&[slug]) {
        Some(s) => s,
        None => {
            eprintln!(
                "{} Not in a registered project.",
                "Error:".red().bold()
            );
            std::process::exit(1);
        }
    };

    let msg = message.unwrap_or("Session ended.");
    append_entry(&slug, "save", Some(msg));
    println!("{} Session saved for \"{}\"", "OK".green().bold(), slug);
}

pub fn history(slug: Option<&str>, limit: usize) {
    let resolved = resolve_project_slug(&[slug]);

    let dir = session_dir();
    let mut files: Vec<_> = match fs::read_dir(&dir) {
        Ok(entries) => entries
            .filter_map(|e| e.ok())
            .filter(|e| e.path().extension().is_some_and(|ext| ext == "jsonl"))
            .map(|e| e.path())
            .collect(),
        Err(_) => Vec::new(),
    };
    files.sort();
    files.reverse();

    let mut entries = Vec::new();
    for file in &files {
        if entries.len() >= limit {
            break;
        }
        if let Ok(f) = fs::File::open(file) {
            let lines: Vec<String> = std::io::BufReader::new(f)
                .lines()
                .map_while(|l| l.ok())
                .collect();
            for line in lines.iter().rev() {
                if entries.len() >= limit {
                    break;
                }
                if let Ok(v) = serde_json::from_str::<serde_json::Value>(line) {
                    if let Some(ref s) = resolved {
                        if v.get("slug").and_then(|v| v.as_str()) != Some(s) {
                            continue;
                        }
                    }
                    entries.push(v);
                }
            }
        }
    }

    if entries.is_empty() {
        println!("{}", "No session history found.".dimmed());
        return;
    }

    for entry in &entries {
        let ts = entry.get("timestamp").and_then(|v| v.as_str()).unwrap_or("?");
        let s = entry.get("slug").and_then(|v| v.as_str()).unwrap_or("?");
        let event = entry.get("event").and_then(|v| v.as_str()).unwrap_or("?");
        let msg = entry
            .get("message")
            .and_then(|v| v.as_str())
            .unwrap_or("");

        print!("  {} [{}] {}", ts.dimmed(), event.cyan(), s.bold());
        if !msg.is_empty() {
            print!("  {}", msg);
        }
        println!();
    }
}

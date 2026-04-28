use graphmind_config::{paths, Registry, resolve_project_slug};
use colored::Colorize;
use graphmind_db::builder::{BuildOptions, GraphBuilder};
use graphmind_db::queries::GraphQueries;
use notify_debouncer_mini::{new_debouncer, DebouncedEventKind};
use serde_json::json;
use std::path::Path;
use std::sync::mpsc;
use std::time::Duration;

pub fn build(slug: Option<&str>, all: bool, full: bool, watch: bool) {
    if watch {
        let slug = match resolve_project_slug(&[slug]) {
            Some(s) => s,
            None => {
                eprintln!("{} No project specified", "Error:".red().bold());
                std::process::exit(1);
            }
        };
        watch_project(&slug);
        return;
    }

    if all {
        let projects = Registry::list();
        if projects.is_empty() {
            eprintln!("{} No projects registered", "Error:".red().bold());
            std::process::exit(1);
        }
        for p in &projects {
            build_single(&p.slug, full);
        }
        return;
    }

    let slug = match resolve_project_slug(&[slug]) {
        Some(s) => s,
        None => {
            eprintln!(
                "{} No project specified and none could be resolved. Use 'graphmind register' first.",
                "Error:".red().bold()
            );
            std::process::exit(1);
        }
    };

    build_single(&slug, full);
}

fn build_single(slug: &str, full: bool) {
    let project = match Registry::get(slug) {
        Some(p) => p,
        None => {
            eprintln!("{} Project {} not found", "Error:".red().bold(), slug);
            return;
        }
    };

    println!(
        "{} Building {} at {}",
        ">>".cyan().bold(),
        slug.cyan(),
        project.path.dimmed()
    );

    let db_path = paths::graph_db_path(slug);
    let cache_dir = paths::cache_dir_path(slug);
    let db_path_str = db_path.to_string_lossy().to_string();
    let cache_dir_str = cache_dir.to_string_lossy().to_string();

    let mut builder = GraphBuilder::new(&db_path_str, &cache_dir_str);

    let mut options = BuildOptions {
        full,
        ..BuildOptions::default()
    };
    for e in &project.exclude {
        if !options.exclude.contains(e) {
            options.exclude.push(e.clone());
        }
    }

    let config = Registry::get_config();
    for e in &config.global_exclude {
        if !options.exclude.contains(e) {
            options.exclude.push(e.clone());
        }
    }

    let result = builder.build(&project.path, &options);

    // Collect stats for meta.json
    let queries = GraphQueries::new(builder.database());
    let stats = queries.stats();
    let langs = queries.language_breakdown();

    let lang_names: Vec<String> = langs.iter().map(|l| l.language.clone()).collect();

    // Update project config with build info
    Registry::update_project(slug, |p| {
        p.last_build = Some(chrono::Utc::now().to_rfc3339());
        p.languages.clone_from(&lang_names);
    });

    // Write meta.json
    let meta = json!({
        "slug": slug,
        "path": project.path,
        "last_build": chrono::Utc::now().to_rfc3339(),
        "stats": {
            "symbols": stats.symbols,
            "edges": stats.edges,
            "files": stats.files,
        },
        "languages": langs.iter().map(|l| json!({
            "language": l.language,
            "count": l.count,
        })).collect::<Vec<_>>(),
        "build_result": {
            "files_processed": result.files_processed,
            "symbols_found": result.symbols_found,
            "edges_created": result.edges_created,
            "skipped": result.skipped,
            "deleted": result.deleted,
            "duration_ms": result.duration_ms,
        }
    });
    let meta_path = paths::meta_path(slug);
    std::fs::write(meta_path, serde_json::to_string_pretty(&meta).unwrap_or_default()).ok();

    println!(
        "{} {} | {} symbols | {} edges | {} files processed | {} skipped | {} deleted | {}ms",
        "OK".green().bold(),
        slug.cyan(),
        result.symbols_found.to_string().green(),
        result.edges_created.to_string().green(),
        result.files_processed.to_string().green(),
        result.skipped.to_string().yellow(),
        result.deleted.to_string().red(),
        result.duration_ms
    );
}

fn watch_project(slug: &str) {
    let project = match Registry::get(slug) {
        Some(p) => p,
        None => {
            eprintln!("{} Project {} not found", "Error:".red().bold(), slug);
            std::process::exit(1);
        }
    };

    println!(
        "{} Watching {} at {}",
        ">>".cyan().bold(),
        slug.cyan(),
        project.path.dimmed()
    );

    // Initial build
    build_single(slug, false);

    let (tx, rx) = mpsc::channel();
    let mut debouncer = new_debouncer(Duration::from_secs(2), tx)
        .expect("Failed to create file watcher");

    debouncer
        .watcher()
        .watch(Path::new(&project.path), notify::RecursiveMode::Recursive)
        .expect("Failed to watch project directory");

    println!("{} Watching for changes... (Ctrl+C to stop)", ">>".cyan().bold());

    loop {
        match rx.recv() {
            Ok(Ok(events)) => {
                let dominated_exts = [
                    ".ts", ".tsx", ".js", ".jsx", ".mjs", ".py", ".go", ".rs", ".rb", ".tf", ".yml", ".yaml",
                ];
                let has_relevant = events.iter().any(|e| {
                    if e.kind != DebouncedEventKind::Any { return false; }
                    let path_str = e.path.to_string_lossy();
                    dominated_exts.iter().any(|ext| path_str.ends_with(ext))
                });
                if has_relevant {
                    println!("\n{} Change detected, rebuilding...", ">>".cyan().bold());
                    build_single(slug, false);
                    println!("{} Watching for changes...", ">>".cyan().bold());
                }
            }
            Ok(Err(e)) => {
                eprintln!("{} Watch error: {:?}", "Error:".red().bold(), e);
            }
            Err(e) => {
                eprintln!("{} Channel error: {}", "Error:".red().bold(), e);
                break;
            }
        }
    }
}

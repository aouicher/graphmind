use graphmind_config::{paths, Registry, resolve_project_slug};
use graphmind_config::config::{EmbeddingMode, load_config};
use colored::Colorize;
use graphmind_db::builder::{BuildOptions, GraphBuilder};
use graphmind_db::queries::GraphQueries;
use graphmind_embeddings::factory::create_engine;
use graphmind_embeddings::store::{EmbeddingStore, NewEmbeddingRow, float32_to_bytes};
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

    // Embedding step
    let global_config = load_config();
    if global_config.embedding.mode != EmbeddingMode::Disabled {
        run_embedding_step(slug, &queries, &global_config.embedding);
    }

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

fn run_embedding_step(
    slug: &str,
    queries: &GraphQueries,
    config: &graphmind_config::config::EmbeddingConfig,
) {
    let engine = match create_engine(config) {
        Ok(e) => e,
        Err(e) => {
            eprintln!("{} Embeddings: {}", "Warning:".yellow().bold(), e);
            return;
        }
    };

    let emb_db_path = paths::embedding_db_path(slug);
    let store = match EmbeddingStore::open(&emb_db_path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("{} Embedding store: {}", "Warning:".yellow().bold(), e);
            return;
        }
    };

    // Check model compatibility — re-index if model changed
    let current_model = format!("{}:{}", engine.provider_name(), engine.model_id());
    if let Some(stored_model) = store.get_meta("model") {
        if stored_model != current_model {
            println!(
                "{} Embedding model changed ({} → {}), re-indexing...",
                ">>".cyan().bold(),
                stored_model.dimmed(),
                current_model.cyan()
            );
            store.clear().ok();
        }
    }
    store.set_meta("model", &current_model).ok();

    let indexed_files = store.files_indexed();
    let all_symbols = queries.all_symbols();

    // Find files that need embedding (new or re-processed by graph build)
    let files_in_graph: std::collections::HashSet<String> =
        all_symbols.iter().map(|s| s.file.clone()).collect();

    // Remove embeddings for files no longer in graph
    for old_file in &indexed_files {
        if !files_in_graph.contains(old_file) {
            store.delete_by_file(old_file).ok();
        }
    }

    // Collect symbols from files not yet indexed
    let new_symbols: Vec<_> = all_symbols
        .iter()
        .filter(|s| !indexed_files.contains(&s.file))
        .filter(|s| s.content.is_some() || s.signature.is_some())
        .collect();

    if new_symbols.is_empty() {
        return;
    }

    println!(
        "{} Embedding {} symbols across {} new files...",
        ">>".cyan().bold(),
        new_symbols.len(),
        new_symbols.iter().map(|s| &s.file).collect::<std::collections::HashSet<_>>().len()
    );

    // Build texts for embedding
    let texts: Vec<String> = new_symbols
        .iter()
        .map(|s| {
            let mut t = format!("{} {} ({})", s.kind, s.name, s.file);
            if let Some(sig) = &s.signature {
                t.push_str(&format!("\n{sig}"));
            }
            if let Some(content) = &s.content {
                let truncated: String = content.chars().take(512).collect();
                t.push_str(&format!("\n{truncated}"));
            }
            t
        })
        .collect();

    const CHUNK_SIZE: usize = 256;
    for (chunk_idx, chunk) in new_symbols.chunks(CHUNK_SIZE).enumerate() {
        let chunk_texts: Vec<String> = chunk
            .iter()
            .map(|s| {
                let idx = chunk_idx * CHUNK_SIZE + chunk.iter().position(|x| x.id == s.id).unwrap();
                texts[idx].clone()
            })
            .collect();
        let text_refs: Vec<&str> = chunk_texts.iter().map(|t| t.as_str()).collect();
        match engine.embed_batch(&text_refs) {
            Ok(embeddings) => {
                let rows: Vec<NewEmbeddingRow> = chunk
                    .iter()
                    .zip(embeddings.iter())
                    .map(|(sym, emb)| NewEmbeddingRow {
                        symbol_name: sym.name.clone(),
                        symbol_kind: sym.kind.clone(),
                        file: sym.file.clone(),
                        text: chunk_texts[chunk.iter().position(|x| x.id == sym.id).unwrap()].clone(),
                        embedding: float32_to_bytes(emb),
                    })
                    .collect();
                if let Err(e) = store.insert_batch(&rows) {
                    eprintln!("{} Embedding insert: {}", "Warning:".yellow().bold(), e);
                } else {
                    println!(
                        "{} Embedded {}/{} symbols ({})",
                        "OK".green().bold(),
                        rows.len(),
                        new_symbols.len(),
                        current_model.dimmed()
                    );
                }
            }
            Err(e) => {
                eprintln!("{} Embedding batch: {}", "Warning:".yellow().bold(), e);
            }
        }
    }
}

pub fn embed_only(slug: Option<&str>, all: bool) {
    let global_config = load_config();
    if global_config.embedding.mode == EmbeddingMode::Disabled {
        eprintln!("{} Embeddings are disabled. Configure in ~/.graphmind/config.json", "Error:".red().bold());
        std::process::exit(1);
    }

    if all {
        let projects = Registry::list();
        if projects.is_empty() {
            eprintln!("{} No projects registered", "Error:".red().bold());
            std::process::exit(1);
        }
        let mut count = 0;
        for p in &projects {
            let db_path = paths::graph_db_path(&p.slug);
            if db_path.exists() {
                embed_single(&p.slug, &global_config.embedding);
                count += 1;
            }
        }
        if count == 0 {
            println!("{} No projects with a graph found. Run 'graphmind build' first.", "!".yellow());
        }
        return;
    }

    let slug = match resolve_project_slug(&[slug]) {
        Some(s) => s,
        None => {
            eprintln!("{} No project specified", "Error:".red().bold());
            std::process::exit(1);
        }
    };

    let db_path = paths::graph_db_path(&slug);
    if !db_path.exists() {
        eprintln!("{} No graph for {}. Run 'graphmind build' first.", "Error:".red().bold(), slug);
        std::process::exit(1);
    }

    embed_single(&slug, &global_config.embedding);
}

fn embed_single(slug: &str, emb_config: &graphmind_config::config::EmbeddingConfig) {
    let db_path = paths::graph_db_path(slug);
    let db_path_str = db_path.to_string_lossy().to_string();
    let db = graphmind_db::schema::init_database(&db_path_str).unwrap_or_else(|e| {
        eprintln!("{} Failed to open database: {}", "Error:".red().bold(), e);
        std::process::exit(1);
    });
    let queries = GraphQueries::new(&db);
    run_embedding_step(slug, &queries, emb_config);
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

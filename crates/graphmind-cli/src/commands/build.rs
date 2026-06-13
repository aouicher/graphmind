use graphmind_config::{paths, Registry, resolve_project_slug};
use graphmind_config::config::{EmbeddingMode, load_config};
use graphmind_api_client::{ApiClient, EmbedChunk, is_remote_embed};
use colored::Colorize;
use graphmind_db::builder::{BuildOptions, BuildResult, GraphBuilder};
use graphmind_db::queries::GraphQueries;
use graphmind_db::schema::init_database;
use graphmind_embeddings::factory::create_engine;
use graphmind_embeddings::store::{EmbeddingStore, NewEmbeddingRow, float32_to_bytes};
use notify_debouncer_mini::{new_debouncer, DebouncedEventKind};
use serde_json::json;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::time::Duration;

pub fn build(slug: Option<&str>, all: bool, full: bool, reset: bool, watch: bool) {
    if watch && all {
        let projects = Registry::list();
        if projects.is_empty() {
            eprintln!("{} No projects registered", "Error:".red().bold());
            std::process::exit(1);
        }
        let handles: Vec<_> = projects
            .into_iter()
            .map(|p| {
                std::thread::spawn(move || watch_project(&p.slug))
            })
            .collect();
        for h in handles {
            let _ = h.join();
        }
        return;
    }

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
            build_single(&p.slug, full, reset, true);
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

    build_single(&slug, full, reset, false);
}

fn build_single(slug: &str, full: bool, reset: bool, is_all: bool) {
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

    if !reset && graphmind_db::schema::schema_needs_reset(&db_path_str) {
        println!(
            "{} Graph schema is outdated. Run {} to reindex.",
            "Warning:".yellow().bold(),
            "graphmind build --reset".bold()
        );
    }

    if reset {
        std::fs::remove_file(&db_path).ok();
        std::fs::remove_dir_all(&cache_dir).ok();
        println!("{} DB and cache cleared", ">>".cyan().bold());
    }

    let mut builder = GraphBuilder::new(&db_path_str, &cache_dir_str);

    let mut options = BuildOptions {
        full: full || reset,
        reset,
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

    // Embedding + remote sync step
    let global_config = load_config();
    if is_remote_embed(&global_config) {
        run_remote_embed_step(slug, &queries, &global_config);
    } else if global_config.embedding.mode != EmbeddingMode::Disabled {
        run_embedding_step(slug, &queries, &global_config.embedding, None);
    }
    if graphmind_api_client::is_remote_full(&global_config) {
        run_remote_sync_step(slug, &queries, &global_config);
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

    if reset && !is_all {
        let stale_others: Vec<_> = Registry::list()
            .into_iter()
            .filter(|p| {
                if p.slug == slug { return false; }
                let db = paths::graph_db_path(&p.slug);
                graphmind_db::schema::schema_needs_reset(db.to_str().unwrap_or(""))
            })
            .collect();
        if !stale_others.is_empty() {
            println!();
            println!(
                "  {} {} other project(s) may need a reset too.",
                "Note:".yellow().bold(),
                stale_others.len()
            );
            println!(
                "  Run {} to reindex all projects.",
                "graphmind build --reset --all".bold()
            );
        }
    }
}

fn run_embedding_step(
    slug: &str,
    queries: &GraphQueries,
    config: &graphmind_config::config::EmbeddingConfig,
    cancel: Option<Arc<AtomicBool>>,
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
        if cancel.as_ref().map(|c| c.load(Ordering::Relaxed)).unwrap_or(false) {
            eprintln!("Embedding cancelled.");
            return;
        }
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
    run_embedding_step(slug, &queries, emb_config, None);
}

fn build_changed_files(slug: &str, changed_paths: &[PathBuf]) -> BuildResult {
    let project = match Registry::get(slug) {
        Some(p) => p,
        None => return BuildResult::default(),
    };

    let project_path = PathBuf::from(&project.path);
    let rel_paths: Vec<String> = changed_paths
        .iter()
        .filter_map(|p| p.strip_prefix(&project_path).ok().map(|r| r.to_string_lossy().to_string()))
        .collect();

    if rel_paths.is_empty() {
        return BuildResult::default();
    }

    let db_path = paths::graph_db_path(slug);
    let cache_dir = paths::cache_dir_path(slug);
    let mut builder = GraphBuilder::new(
        &db_path.to_string_lossy(),
        &cache_dir.to_string_lossy(),
    );

    let mut options = BuildOptions {
        only_files: Some(rel_paths),
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

    // Update registry + meta.json
    let queries = GraphQueries::new(builder.database());
    let stats = queries.stats();
    let langs = queries.language_breakdown();
    let lang_names: Vec<String> = langs.iter().map(|l| l.language.clone()).collect();

    Registry::update_project(slug, |p| {
        p.last_build = Some(chrono::Utc::now().to_rfc3339());
        p.languages.clone_from(&lang_names);
    });

    let meta = json!({
        "slug": slug,
        "path": project.path,
        "last_build": chrono::Utc::now().to_rfc3339(),
        "stats": { "symbols": stats.symbols, "edges": stats.edges, "files": stats.files },
        "languages": langs.iter().map(|l| json!({"language": l.language, "count": l.count})).collect::<Vec<_>>(),
        "build_result": {
            "files_processed": result.files_processed,
            "symbols_found": result.symbols_found,
            "edges_created": result.edges_created,
            "skipped": result.skipped,
            "deleted": result.deleted,
            "duration_ms": result.duration_ms,
        }
    });
    std::fs::write(paths::meta_path(slug), serde_json::to_string_pretty(&meta).unwrap_or_default()).ok();

    result
}

fn run_embedding_for_files(
    slug: &str,
    changed_rel_paths: &[String],
    config: &graphmind_config::config::EmbeddingConfig,
) {
    if changed_rel_paths.is_empty() {
        return;
    }

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

    let graph_db_path = paths::graph_db_path(slug);
    let db = match init_database(&graph_db_path.to_string_lossy()) {
        Ok(d) => d,
        Err(e) => {
            eprintln!("{} Graph DB: {}", "Warning:".yellow().bold(), e);
            return;
        }
    };
    let queries = GraphQueries::new(&db);

    // Delete old embeddings and collect new symbols for changed files
    let mut symbols = Vec::new();
    for file in changed_rel_paths {
        store.delete_by_file(file).ok();
        let file_symbols = queries.symbols_in_file(file);
        symbols.extend(file_symbols);
    }

    let symbols: Vec<_> = symbols.iter()
        .filter(|s| s.content.is_some() || s.signature.is_some())
        .collect();

    if symbols.is_empty() {
        return;
    }

    let texts: Vec<String> = symbols.iter().map(|s| {
        let mut t = format!("{} {} ({})", s.kind, s.name, s.file);
        if let Some(sig) = &s.signature { t.push_str(&format!("\n{sig}")); }
        if let Some(content) = &s.content {
            let truncated: String = content.chars().take(512).collect();
            t.push_str(&format!("\n{truncated}"));
        }
        t
    }).collect();

    const CHUNK_SIZE: usize = 256;
    let mut total_embedded = 0usize;
    for (i, chunk) in symbols.chunks(CHUNK_SIZE).enumerate() {
        let chunk_texts: Vec<&str> = texts[i * CHUNK_SIZE..][..chunk.len()].iter().map(|t| t.as_str()).collect();
        match engine.embed_batch(&chunk_texts) {
            Ok(embeddings) => {
                let rows: Vec<NewEmbeddingRow> = chunk.iter().zip(embeddings.iter()).map(|(sym, emb)| {
                    NewEmbeddingRow {
                        symbol_name: sym.name.clone(),
                        symbol_kind: sym.kind.clone(),
                        file: sym.file.clone(),
                        text: chunk_texts[chunk.iter().position(|x| x.id == sym.id).unwrap()].to_string(),
                        embedding: float32_to_bytes(emb),
                    }
                }).collect();
                if let Err(e) = store.insert_batch(&rows) {
                    eprintln!("{} Embedding insert: {}", "Warning:".yellow().bold(), e);
                } else {
                    total_embedded += rows.len();
                }
            }
            Err(e) => eprintln!("{} Embedding batch: {}", "Warning:".yellow().bold(), e),
        }
    }

    if total_embedded > 0 {
        println!("{} Embeddings updated ({} symbols)", ">>".cyan().bold(), total_embedded);
    }
}

fn build_embed_chunks_from_queries(_slug: &str, queries: &GraphQueries) -> Vec<EmbedChunk> {
    queries.all_symbols()
        .into_iter()
        .filter(|s| s.content.is_some() || s.signature.is_some())
        .map(|s| {
            let mut text = format!("{} {} ({})", s.kind, s.name, s.file);
            if let Some(sig) = &s.signature { text.push_str(&format!("\n{sig}")); }
            if let Some(content) = &s.content {
                text.push_str(&format!("\n{}", content.chars().take(512).collect::<String>()));
            }
            EmbedChunk {
                symbol_name: s.name.clone(),
                content: text,
                file_path: Some(s.file.clone()),
            }
        })
        .collect()
}

fn run_remote_embed_step(slug: &str, queries: &GraphQueries, config: &graphmind_config::config::GlobalConfig) {
    let client = match ApiClient::from_config(config) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("{} Remote embed: {}", "Warning:".yellow().bold(), e);
            return;
        }
    };

    let chunks = build_embed_chunks_from_queries(slug, queries);
    if chunks.is_empty() {
        return;
    }

    println!("{} Remote embedding {} symbols...", ">>".cyan().bold(), chunks.len());

    match client.embed_chunks(slug, chunks) {
        Ok(result) => println!(
            "{} Remote embed: {} stored, {} skipped",
            "OK".green().bold(), result.stored, result.skipped
        ),
        Err(e) => eprintln!("{} Remote embed: {}", "Warning:".yellow().bold(), e),
    }
}

fn run_remote_embed_for_files(slug: &str, changed_rel_paths: &[String], config: &graphmind_config::config::GlobalConfig) {
    if changed_rel_paths.is_empty() {
        return;
    }

    let client = match ApiClient::from_config(config) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("{} Remote embed: {}", "Warning:".yellow().bold(), e);
            return;
        }
    };

    let graph_db_path = paths::graph_db_path(slug);
    let db = match init_database(&graph_db_path.to_string_lossy()) {
        Ok(d) => d,
        Err(e) => {
            eprintln!("{} Graph DB: {}", "Warning:".yellow().bold(), e);
            return;
        }
    };
    let queries = GraphQueries::new(&db);

    let chunks: Vec<EmbedChunk> = changed_rel_paths.iter()
        .flat_map(|file| queries.symbols_in_file(file))
        .filter(|s| s.content.is_some() || s.signature.is_some())
        .map(|s| {
            let mut text = format!("{} {} ({})", s.kind, s.name, s.file);
            if let Some(sig) = &s.signature { text.push_str(&format!("\n{sig}")); }
            if let Some(content) = &s.content {
                text.push_str(&format!("\n{}", content.chars().take(512).collect::<String>()));
            }
            EmbedChunk {
                symbol_name: s.name.clone(),
                content: text,
                file_path: Some(s.file.clone()),
            }
        })
        .collect();

    if chunks.is_empty() {
        return;
    }

    match client.embed_chunks(slug, chunks) {
        Ok(result) => println!(
            "{} Remote embeddings updated ({} stored, {} skipped)",
            ">>".cyan().bold(), result.stored, result.skipped
        ),
        Err(e) => eprintln!("{} Remote embed: {}", "Warning:".yellow().bold(), e),
    }
}

fn run_remote_sync_step(slug: &str, queries: &GraphQueries, config: &graphmind_config::config::GlobalConfig) {
    use graphmind_api_client::{ApiClient, GraphSyncPayload, SyncSymbol, SyncCallEdge};

    let client = match ApiClient::from_config(config) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("{} Remote sync: {}", "Warning:".yellow().bold(), e);
            return;
        }
    };

    // Build symbols payload from graph DB
    let symbols: Vec<SyncSymbol> = queries.all_symbols()
        .into_iter()
        .map(|s| SyncSymbol {
            name: s.name,
            kind: s.kind,
            file_path: s.file,
            line_start: s.line_start,
            line_end: if s.line_end > 0 { Some(s.line_end) } else { None },
            source_code: s.content,
            signature: s.signature,
            parent_name: None,
            qualified_name: None,
        })
        .collect();

    // Build call edges payload — query directly from the graph DB
    let call_edges: Vec<SyncCallEdge> = {
        let db_path = paths::graph_db_path(slug);
        match init_database(&db_path.to_string_lossy()) {
            Ok(db) => {
                let mut stmt = db.prepare(
                    "SELECT s1.name, s1.file, s2.name, s2.file \
                     FROM edges e \
                     JOIN symbols s1 ON s1.id = e.from_id \
                     JOIN symbols s2 ON s2.id = e.to_id \
                     WHERE e.kind = 'calls'"
                ).unwrap_or_else(|_| panic!("prepare edges query"));
                stmt.query_map([], |row| {
                    Ok(SyncCallEdge {
                        caller_name: row.get(0)?,
                        caller_file: row.get(1)?,
                        callee_name: row.get(2)?,
                        callee_file: row.get(3)?,
                    })
                })
                .unwrap_or_else(|_| panic!("query edges"))
                .filter_map(|r| r.ok())
                .collect()
            }
            Err(e) => {
                eprintln!("{} Remote sync DB: {}", "Warning:".yellow().bold(), e);
                return;
            }
        }
    };

    let since = config.remote.last_sync_at.clone();
    let is_incremental = since.is_some();

    println!(
        "{} Remote sync: {} symbols, {} edges{}...",
        ">>".cyan().bold(),
        symbols.len(),
        call_edges.len(),
        if is_incremental { " (incremental)" } else { " (full)" }
    );

    let payload = GraphSyncPayload {
        project_slug: slug.to_string(),
        since,
        symbols,
        call_edges,
        file_imports: vec![],
        cross_project_links: None,
        event_listeners: None,
    };

    match client.sync_graph(&payload) {
        Ok(result) => {
            println!(
                "{} Remote sync: {} symbols, {} edges, version {}",
                "OK".green().bold(), result.symbols, result.edges, result.version
            );
            // Persist last_sync_at so next build is incremental
            let mut updated_config = graphmind_config::config::load_config();
            updated_config.remote.last_sync_at = Some(result.synced_at);
            graphmind_config::config::save_config(&updated_config);
        }
        Err(e) => eprintln!("{} Remote sync: {}", "Warning:".yellow().bold(), e),
    }
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

    // Initial full build
    build_single(slug, false, false, false);

    let (tx, rx) = mpsc::channel();
    let mut debouncer = new_debouncer(Duration::from_millis(500), tx)
        .expect("Failed to create file watcher");

    debouncer
        .watcher()
        .watch(Path::new(&project.path), notify::RecursiveMode::Recursive)
        .expect("Failed to watch project directory");

    println!("{} Watching for changes... (Ctrl+C to stop)", ">>".cyan().bold());

    // Extensions to watch — matches LANGUAGE_MAP in builder.rs
    let watched_exts = [
        ".ts", ".tsx", ".js", ".jsx", ".mjs", ".py", ".go", ".rs", ".rb",
        ".tf", ".tfvars", ".yml", ".yaml", ".md", ".c", ".h", ".m", ".mm",
        ".java", ".php", ".swift", ".sh", ".bash", ".zsh", ".pl", ".pm",
        ".html", ".htm", ".toml", ".sql", ".cpp", ".cc", ".cxx", ".hpp",
        ".hh", ".cs", ".kt", ".kts", ".dart", ".scala", ".sc", ".r", ".R",
        ".graphql", ".gql", ".ps1", ".psm1",
    ];

    // Directory segments to ignore
    let excluded_dirs = [
        "node_modules", "dist", "build", "out", ".git", ".next", ".nuxt",
        ".turbo", "coverage", "__pycache__", ".venv", "venv", "env",
        "vendor", "target", "tmp", "log", "cdk.out", ".terraform", ".serverless",
    ];

    loop {
        match rx.recv() {
            Ok(Ok(events)) => {
                let changed: Vec<PathBuf> = events.iter()
                    .filter(|e| e.kind == DebouncedEventKind::Any)
                    .map(|e| e.path.clone())
                    .filter(|p| {
                        let s = p.to_string_lossy();
                        // Must have a watched extension
                        let has_ext = watched_exts.iter().any(|ext| s.ends_with(ext));
                        // Must not be inside an excluded directory
                        let is_excluded = excluded_dirs.iter().any(|dir| {
                            s.contains(&format!("/{dir}/")) || s.contains(&format!("\\{dir}\\"))
                        });
                        has_ext && !is_excluded
                    })
                    .collect::<std::collections::HashSet<_>>()
                    .into_iter()
                    .collect();

                if changed.is_empty() {
                    continue;
                }

                let display: Vec<String> = changed.iter()
                    .filter_map(|p| p.strip_prefix(&project.path).ok().map(|r| r.to_string_lossy().to_string()))
                    .collect();

                println!(
                    "\n{} Change detected ({} file{}):",
                    ">>".cyan().bold(),
                    changed.len(),
                    if changed.len() == 1 { "" } else { "s" }
                );
                for f in &display {
                    println!("     {}", f.dimmed());
                }

                let result = build_changed_files(slug, &changed);

                println!(
                    "{} {} file{} | {} symbols | {} edges | {}ms",
                    "OK".green().bold(),
                    result.files_processed,
                    if result.files_processed == 1 { "" } else { "s" },
                    result.symbols_found.to_string().green(),
                    result.edges_created.to_string().green(),
                    result.duration_ms
                );

                // Incremental embedding update
                let global_config = load_config();
                if is_remote_embed(&global_config) {
                    run_remote_embed_for_files(slug, &display, &global_config);
                } else if global_config.embedding.mode != EmbeddingMode::Disabled {
                    run_embedding_for_files(slug, &display, &global_config.embedding);
                }

                println!("{} Watching for changes...", ">>".cyan().bold());
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

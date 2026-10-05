use graphmind_config::{paths, Registry, resolve_project_slug};
use graphmind_config::config::{EmbeddingMode, load_config};
use colored::Colorize;
use graphmind_db::builder::{BuildOptions, BuildResult, GraphBuilder};
use graphmind_db::queries::{GraphQueries, SymbolRow};
use graphmind_db::schema::init_database;
use graphmind_embeddings::factory::create_engine;
use graphmind_embeddings::engine::EmbeddingEngine;
use graphmind_embeddings::store::EmbeddingStore;
use graphmind_embeddings::sync::{self, EmbedInput, symbol_text};
use notify_debouncer_mini::{new_debouncer, DebouncedEventKind};
use serde_json::json;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
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

    // Embedding step
    let global_config = load_config();
    if global_config.embedding.mode != EmbeddingMode::Disabled {
        // Failures are reported above and retried on the next build; the graph itself is fine,
        // so they don't fail the build (it runs from git hooks).
        run_embedding_step(slug, &queries, &global_config.embedding, None);
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

/// Outcome of an embedding step, for the exit code of `graphmind embed`.
#[derive(Default)]
struct EmbedOutcome {
    failed: usize,
}

/// Open the engine and store, and re-index if the model changed. `None` (after a warning) when
/// embeddings can't run at all.
fn open_embedding(
    slug: &str,
    config: &graphmind_config::config::EmbeddingConfig,
) -> Option<(Box<dyn EmbeddingEngine>, EmbeddingStore, String)> {
    let engine = match create_engine(config) {
        Ok(e) => e,
        Err(e) => {
            eprintln!("{} Embeddings: {}", "Warning:".yellow().bold(), e);
            return None;
        }
    };

    let emb_db_path = paths::embedding_db_path(slug);
    let store = match EmbeddingStore::open(&emb_db_path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("{} Embedding store: {}", "Warning:".yellow().bold(), e);
            return None;
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
    Some((engine, store, current_model))
}

fn embed_inputs(symbols: &[SymbolRow]) -> Vec<EmbedInput> {
    symbols
        .iter()
        .filter(|s| s.content.is_some() || s.signature.is_some())
        .map(|s| EmbedInput {
            symbol_name: s.name.clone(),
            symbol_kind: s.kind.clone(),
            file: s.file.clone(),
            text: symbol_text(&s.kind, &s.name, &s.file, s.signature.as_deref(), s.content.as_deref()),
        })
        .collect()
}

/// Bring the store in line with `wanted` (only rows of `scope` files, if given): drop stale
/// rows, embed new or edited symbols, and report what could not be embedded.
fn sync_embeddings(
    engine: &dyn EmbeddingEngine,
    store: &EmbeddingStore,
    model: &str,
    wanted: Vec<EmbedInput>,
    scope: Option<&[String]>,
    config: &graphmind_config::config::EmbeddingConfig,
    cancel: Option<Arc<AtomicBool>>,
) -> EmbedOutcome {
    let plan = sync::plan(store, wanted, scope);
    if let Err(e) = store.delete_ids(&plan.stale_ids) {
        eprintln!("{} Embedding cleanup: {}", "Warning:".yellow().bold(), e);
    }
    if plan.to_embed.is_empty() {
        return EmbedOutcome::default();
    }

    let files: std::collections::HashSet<_> = plan.to_embed.iter().map(|i| &i.file).collect();
    println!(
        "{} Embedding {} symbols across {} files...",
        ">>".cyan().bold(),
        plan.to_embed.len(),
        files.len()
    );

    let summary = sync::embed_and_store(
        engine,
        store,
        &plan.to_embed,
        config.effective_batch_size(),
        cancel.as_deref(),
        |s| {
            println!(
                "{} Embedded {}/{} symbols ({})",
                "OK".green().bold(),
                s.embedded,
                s.requested,
                model.dimmed()
            );
        },
    );

    if summary.cancelled {
        eprintln!("Embedding cancelled.");
    }
    if summary.failed > 0 {
        eprintln!(
            "{} {} of {} symbols were not embedded: {}",
            "Warning:".yellow().bold(),
            summary.failed,
            summary.requested,
            summary.first_error.as_deref().unwrap_or("unknown error")
        );
        eprintln!(
            "  They are retried on the next build. For a slow local model server, raise \
             embedding.request_timeout_secs or lower embedding.batch_size in ~/.graphmind/config.json."
        );
    }
    EmbedOutcome { failed: summary.failed }
}

fn run_embedding_step(
    slug: &str,
    queries: &GraphQueries,
    config: &graphmind_config::config::EmbeddingConfig,
    cancel: Option<Arc<AtomicBool>>,
) -> EmbedOutcome {
    let Some((engine, store, model)) = open_embedding(slug, config) else {
        return EmbedOutcome { failed: 1 };
    };
    let wanted = embed_inputs(&queries.all_symbols());
    sync_embeddings(engine.as_ref(), &store, &model, wanted, None, config, cancel)
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
        let mut failed = 0;
        for p in &projects {
            let db_path = paths::graph_db_path(&p.slug);
            if db_path.exists() {
                failed += embed_single(&p.slug, &global_config.embedding).failed;
                count += 1;
            }
        }
        if count == 0 {
            println!("{} No projects with a graph found. Run 'graphmind build' first.", "!".yellow());
        }
        if failed > 0 {
            std::process::exit(1);
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

    if embed_single(&slug, &global_config.embedding).failed > 0 {
        std::process::exit(1);
    }
}

fn embed_single(slug: &str, emb_config: &graphmind_config::config::EmbeddingConfig) -> EmbedOutcome {
    let db_path = paths::graph_db_path(slug);
    let db_path_str = db_path.to_string_lossy().to_string();
    let db = graphmind_db::schema::init_database(&db_path_str).unwrap_or_else(|e| {
        eprintln!("{} Failed to open database: {}", "Error:".red().bold(), e);
        std::process::exit(1);
    });
    let queries = GraphQueries::new(&db);
    run_embedding_step(slug, &queries, emb_config, None)
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

    let Some((engine, store, model)) = open_embedding(slug, config) else {
        return;
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

    let symbols: Vec<_> = changed_rel_paths
        .iter()
        .flat_map(|file| queries.symbols_in_file(file))
        .collect();
    let wanted = embed_inputs(&symbols);
    sync_embeddings(engine.as_ref(), &store, &model, wanted, Some(changed_rel_paths), config, None);
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
                if global_config.embedding.mode != EmbeddingMode::Disabled {
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

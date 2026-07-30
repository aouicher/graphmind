use graphmind_config::{paths, Registry};
use graphmind_config::config::{EmbeddingMode, load_config};
use graphmind_db::builder::{BuildOptions, GraphBuilder};
use graphmind_db::queries::GraphQueries;
use graphmind_db::schema::init_database;
use graphmind_embeddings::factory::create_engine;
use graphmind_embeddings::store::{EmbeddingStore, NewEmbeddingRow, float32_to_bytes};
use serde_json::json;
use std::sync::{Arc, Mutex};
use std::sync::atomic::{AtomicBool, Ordering};
use tauri::{AppHandle, Emitter, State};

use crate::state::AppState;

#[tauri::command]
pub async fn build_project(
    slug: String,
    full: bool,
    app: AppHandle,
    state: State<'_, Mutex<AppState>>,
) -> Result<(), String> {
    let project =
        Registry::get(&slug).ok_or_else(|| format!("Project {slug} not found"))?;

    let cancel = Arc::new(AtomicBool::new(false));
    state.lock().unwrap().cancel_flags.insert(slug.clone(), cancel.clone());

    app.emit("indexing-started", &slug).ok();

    let cancel_clone = cancel.clone();
    let result = tokio::task::spawn_blocking(move || {
        let db_path = paths::graph_db_path(&slug);
        let cache_dir = paths::cache_dir_path(&slug);
        let db_path_str = db_path.to_string_lossy().to_string();
        let cache_dir_str = cache_dir.to_string_lossy().to_string();

        let mut builder = GraphBuilder::new(&db_path_str, &cache_dir_str);

        let mut options = BuildOptions {
            full,
            cancel: Some(cancel_clone.clone()),
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

        // If cancelled, skip metadata update
        if cancel_clone.load(Ordering::Relaxed) {
            return Err(slug);
        }

        let queries = GraphQueries::new(builder.database());
        let stats = queries.stats();
        let langs = queries.language_breakdown();
        let lang_names: Vec<String> = langs.iter().map(|l| l.language.clone()).collect();

        Registry::update_project(&slug, |p| {
            p.last_build = Some(chrono::Utc::now().to_rfc3339());
            p.languages.clone_from(&lang_names);
        });

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
        let meta_path = paths::meta_path(&slug);
        std::fs::write(meta_path, serde_json::to_string_pretty(&meta).unwrap_or_default()).ok();

        Ok(slug)
    })
    .await
    .map_err(|e| format!("Build failed: {e}"))?;

    // If cancelled, clean up and emit cancelled event
    let slug_result = match result {
        Ok(s) => s,
        Err(slug) => {
            state.lock().unwrap().cancel_flags.remove(&slug);
            app.emit("indexing-cancelled", &slug).ok();
            return Ok(());
        }
    };

    // Embedding step — emit separate event so UI can show distinct phase
    let global_config = load_config();
    if global_config.embedding.mode != EmbeddingMode::Disabled {
        // Check cancellation before starting embedding
        if !cancel.load(Ordering::Relaxed) {
            app.emit("embedding-started", &slug_result).ok();
            let slug_clone = slug_result.clone();
            let emb_config = global_config.embedding.clone();
            let cancel_emb = cancel.clone();
            tokio::task::spawn_blocking(move || {
                let db_path = paths::graph_db_path(&slug_clone);
                let db_path_str = db_path.to_string_lossy().to_string();
                if let Ok(db) = graphmind_db::schema::init_database(&db_path_str) {
                    let queries = graphmind_db::queries::GraphQueries::new(&db);
                    run_embedding_step(&slug_clone, &queries, &emb_config, Some(cancel_emb));
                }
            })
            .await
            .map_err(|e| format!("Embed failed: {e}"))?;
            app.emit("embedding-complete", &slug_result).ok();
        }
    }

    state.lock().unwrap().cancel_flags.remove(&slug_result);
    app.emit("indexing-complete", &slug_result).ok();
    Ok(())
}

/// Called from non-command contexts (e.g. auto-build on project add) where no State is available.
pub async fn build_project_uncancelled(slug: String, full: bool, app: AppHandle) -> Result<(), String> {
    let project =
        Registry::get(&slug).ok_or_else(|| format!("Project {slug} not found"))?;

    app.emit("indexing-started", &slug).ok();

    let result = tokio::task::spawn_blocking(move || {
        let db_path = paths::graph_db_path(&slug);
        let cache_dir = paths::cache_dir_path(&slug);
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

        let queries = GraphQueries::new(builder.database());
        let stats = queries.stats();
        let langs = queries.language_breakdown();
        let lang_names: Vec<String> = langs.iter().map(|l| l.language.clone()).collect();

        Registry::update_project(&slug, |p| {
            p.last_build = Some(chrono::Utc::now().to_rfc3339());
            p.languages.clone_from(&lang_names);
        });

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
        let meta_path = paths::meta_path(&slug);
        std::fs::write(meta_path, serde_json::to_string_pretty(&meta).unwrap_or_default()).ok();

        slug
    })
    .await
    .map_err(|e| format!("Build failed: {e}"))?;

    let global_config = load_config();
    if global_config.embedding.mode != EmbeddingMode::Disabled {
        app.emit("embedding-started", &result).ok();
        let slug_clone = result.clone();
        let emb_config = global_config.embedding.clone();
        tokio::task::spawn_blocking(move || {
            let db_path = paths::graph_db_path(&slug_clone);
            let db_path_str = db_path.to_string_lossy().to_string();
            if let Ok(db) = graphmind_db::schema::init_database(&db_path_str) {
                let queries = graphmind_db::queries::GraphQueries::new(&db);
                run_embedding_step(&slug_clone, &queries, &emb_config, None);
            }
        })
        .await
        .map_err(|e| format!("Embed failed: {e}"))?;
        app.emit("embedding-complete", &result).ok();
    }

    app.emit("indexing-complete", &result).ok();
    Ok(())
}

#[tauri::command]
pub async fn build_all_projects(full: bool, app: AppHandle, state: State<'_, Mutex<AppState>>) -> Result<(), String> {
    let projects = Registry::list();
    if projects.is_empty() {
        return Err("No projects registered".to_string());
    }
    // Keep going when a single project fails so one broken repo does not abort
    // the whole batch, and always emit `build-all-complete` so the UI can leave
    // its loading state even when a project errors out or gets cancelled.
    let mut failures: Vec<String> = Vec::new();
    for p in &projects {
        if let Err(e) = build_project(p.slug.clone(), full, app.clone(), state.clone()).await {
            failures.push(format!("{}: {e}", p.slug));
        }
    }
    app.emit("build-all-complete", &failures).ok();
    Ok(())
}

#[tauri::command]
pub async fn cancel_build(
    slug: String,
    state: State<'_, Mutex<AppState>>,
    app: AppHandle,
) -> Result<(), String> {
    let mut locked = state.lock().unwrap();
    if slug.is_empty() {
        // Cancel all in-progress builds
        for (s, flag) in &locked.cancel_flags {
            flag.store(true, Ordering::Relaxed);
            app.emit("indexing-cancelled", s).ok();
        }
        locked.cancel_flags.clear();
    } else if let Some(flag) = locked.cancel_flags.get(&slug) {
        flag.store(true, Ordering::Relaxed);
        app.emit("indexing-cancelled", &slug).ok();
    }
    Ok(())
}

#[tauri::command]
pub async fn embed_projects(slugs: Vec<String>, app: AppHandle) -> Result<(), String> {
    let config = load_config();
    if config.embedding.mode == EmbeddingMode::Disabled {
        return Err("Embeddings are disabled".to_string());
    }

    for slug in &slugs {
        app.emit("embedding-started", slug).ok();

        let slug_clone = slug.clone();
        let emb_config = config.embedding.clone();

        let result = tokio::task::spawn_blocking(move || {
            let db_path = paths::graph_db_path(&slug_clone);
            if !db_path.exists() {
                return Err(format!("No graph for project {slug_clone}"));
            }
            let db_path_str = db_path.to_string_lossy().to_string();
            let db = init_database(&db_path_str)
                .map_err(|e| format!("DB error: {e}"))?;
            let queries = GraphQueries::new(&db);
            run_embedding_step(&slug_clone, &queries, &emb_config, None);
            Ok(slug_clone)
        })
        .await
        .map_err(|e| format!("Embed failed: {e}"))??;

        app.emit("embedding-complete", &result).ok();
    }
    Ok(())
}

fn run_embedding_step(
    slug: &str,
    queries: &GraphQueries,
    config: &graphmind_config::config::EmbeddingConfig,
    cancel: Option<Arc<AtomicBool>>,
) {
    let engine = match create_engine(config) {
        Ok(e) => e,
        Err(_) => return,
    };

    let emb_db_path = paths::embedding_db_path(slug);
    let store = match EmbeddingStore::open(&emb_db_path) {
        Ok(s) => s,
        Err(_) => return,
    };

    let current_model = format!("{}:{}", engine.provider_name(), engine.model_id());
    if let Some(stored_model) = store.get_meta("model") {
        if stored_model != current_model {
            store.clear().ok();
        }
    }
    store.set_meta("model", &current_model).ok();

    let indexed_files = store.files_indexed();
    let all_symbols = queries.all_symbols();

    let files_in_graph: std::collections::HashSet<String> =
        all_symbols.iter().map(|s| s.file.clone()).collect();

    for old_file in &indexed_files {
        if !files_in_graph.contains(old_file) {
            store.delete_by_file(old_file).ok();
        }
    }

    let new_symbols: Vec<_> = all_symbols
        .iter()
        .filter(|s| !indexed_files.contains(&s.file))
        .filter(|s| s.content.is_some() || s.signature.is_some())
        .collect();

    if new_symbols.is_empty() {
        return;
    }

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
        let chunk_texts: Vec<&str> = chunk
            .iter()
            .map(|s| {
                let idx = chunk_idx * CHUNK_SIZE + chunk.iter().position(|x| x.id == s.id).unwrap();
                texts[idx].as_str()
            })
            .collect();
        if let Ok(embeddings) = engine.embed_batch(&chunk_texts) {
            let rows: Vec<NewEmbeddingRow> = chunk
                .iter()
                .zip(embeddings.iter())
                .map(|(sym, emb)| NewEmbeddingRow {
                    symbol_name: sym.name.clone(),
                    symbol_kind: sym.kind.clone(),
                    file: sym.file.clone(),
                    text: chunk_texts[chunk.iter().position(|x| x.id == sym.id).unwrap()].to_string(),
                    embedding: float32_to_bytes(emb),
                })
                .collect();
            store.insert_batch(&rows).ok();
        }
    }
}

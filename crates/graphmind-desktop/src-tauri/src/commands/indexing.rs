use graphmind_config::{paths, Registry};
use graphmind_config::config::{EmbeddingMode, load_config};
use graphmind_db::builder::{BuildOptions, GraphBuilder};
use graphmind_db::queries::GraphQueries;
use graphmind_embeddings::factory::create_engine;
use graphmind_embeddings::store::{EmbeddingStore, NewEmbeddingRow, float32_to_bytes};
use serde_json::json;
use tauri::{AppHandle, Emitter};

#[tauri::command]
pub async fn build_project(slug: String, full: bool, app: AppHandle) -> Result<(), String> {
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

        // Embedding step
        let global_config = load_config();
        if global_config.embedding.mode != EmbeddingMode::Disabled {
            run_embedding_step(&slug, &queries, &global_config.embedding);
        }

        slug
    })
    .await
    .map_err(|e| format!("Build failed: {e}"))?;

    app.emit("indexing-complete", &result).ok();
    Ok(())
}

#[tauri::command]
pub async fn build_all_projects(full: bool, app: AppHandle) -> Result<(), String> {
    let projects = Registry::list();
    if projects.is_empty() {
        return Err("No projects registered".to_string());
    }
    for p in &projects {
        build_project(p.slug.clone(), full, app.clone()).await?;
    }
    Ok(())
}

fn run_embedding_step(
    slug: &str,
    queries: &GraphQueries,
    config: &graphmind_config::config::EmbeddingConfig,
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

    let text_refs: Vec<&str> = texts.iter().map(|t| t.as_str()).collect();

    if let Ok(embeddings) = engine.embed_batch(&text_refs) {
        let rows: Vec<NewEmbeddingRow> = new_symbols
            .iter()
            .zip(embeddings.iter())
            .map(|(sym, emb)| NewEmbeddingRow {
                symbol_name: sym.name.clone(),
                symbol_kind: sym.kind.clone(),
                file: sym.file.clone(),
                text: texts[new_symbols.iter().position(|s| s.id == sym.id).unwrap()].clone(),
                embedding: float32_to_bytes(emb),
            })
            .collect();

        store.insert_batch(&rows).ok();
    }
}

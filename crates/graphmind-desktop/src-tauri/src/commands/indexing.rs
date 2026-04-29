use graphmind_config::{paths, Registry};
use graphmind_db::builder::{BuildOptions, GraphBuilder};
use graphmind_db::queries::GraphQueries;
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

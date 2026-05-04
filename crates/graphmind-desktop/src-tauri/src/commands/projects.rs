use crate::state::AppState;
use crate::types::{GraphStats, ProjectInfo};
use graphmind_config::{paths, Registry};
use std::sync::Mutex;
use tauri::{AppHandle, State};

#[tauri::command]
pub fn list_projects(state: State<Mutex<AppState>>) -> Vec<ProjectInfo> {
    let app_state = state.lock().unwrap();
    let projects = Registry::list();

    projects
        .into_iter()
        .map(|p| {
            let stats = get_stats(&p.slug);
            let is_watching = app_state.watchers.contains_key(&p.slug);
            ProjectInfo {
                slug: p.slug,
                path: p.path,
                last_build: p.last_build,
                languages: p.languages,
                stats,
                is_watching,
            }
        })
        .collect()
}

#[tauri::command]
pub async fn add_project(path: String, app: AppHandle) -> Result<ProjectInfo, String> {
    let project = Registry::register(&path, None, &[]);
    let slug = project.slug.clone();
    let info = ProjectInfo {
        slug: project.slug,
        path: project.path,
        last_build: project.last_build,
        languages: project.languages,
        stats: None,
        is_watching: false,
    };

    let app_clone = app.clone();
    tokio::spawn(async move {
        super::indexing::build_project_uncancelled(slug, true, app_clone).await.ok();
    });

    Ok(info)
}

#[tauri::command]
pub fn remove_project(slug: String) -> Result<bool, String> {
    let graph_dir = paths::graph_dir(&slug);
    if graph_dir.exists() {
        std::fs::remove_dir_all(&graph_dir).ok();
    }
    Ok(Registry::unregister(&slug))
}

#[tauri::command]
pub fn get_project_status(slug: String) -> Result<ProjectInfo, String> {
    let project = Registry::get(&slug).ok_or_else(|| format!("Project {slug} not found"))?;
    let stats = get_stats(&slug);
    Ok(ProjectInfo {
        slug: project.slug,
        path: project.path,
        last_build: project.last_build,
        languages: project.languages,
        stats,
        is_watching: false,
    })
}

fn get_stats(slug: &str) -> Option<GraphStats> {
    let db_path = paths::graph_db_path(slug);
    if !db_path.exists() {
        return None;
    }
    let db_path_str = db_path.to_string_lossy().to_string();
    let db = graphmind_db::schema::init_database(&db_path_str).ok()?;
    let queries = graphmind_db::queries::GraphQueries::new(&db);
    let stats = queries.stats();
    Some(GraphStats {
        symbols: stats.symbols,
        edges: stats.edges,
        files: stats.files,
    })
}

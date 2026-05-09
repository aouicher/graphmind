use crate::state::{AppState, WatcherHandle};
use graphmind_config::{paths, Registry};
use graphmind_db::builder::{BuildOptions, GraphBuilder};
use notify_debouncer_mini::{new_debouncer, DebouncedEventKind};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::Duration;
use tauri::{AppHandle, Emitter, State};

const WATCHED_EXTS: &[&str] = &[
    ".ts", ".tsx", ".js", ".jsx", ".mjs", ".py", ".go", ".rs", ".rb",
    ".tf", ".tfvars", ".yml", ".yaml", ".md", ".c", ".h", ".java", ".php",
    ".swift", ".sh", ".bash", ".toml", ".sql", ".cpp", ".cs", ".kt", ".dart",
    ".graphql", ".gql",
];

const EXCLUDED_DIRS: &[&str] = &[
    "node_modules", "dist", "build", "out", ".git", ".next", ".nuxt",
    ".turbo", "coverage", "__pycache__", ".venv", "venv", "env",
    "vendor", "target", "tmp", "log", "cdk.out", ".terraform", ".serverless",
];

fn is_relevant(path: &Path) -> bool {
    let s = path.to_string_lossy();
    let has_ext = WATCHED_EXTS.iter().any(|ext| s.ends_with(ext));
    let is_excluded = EXCLUDED_DIRS.iter().any(|dir| {
        s.contains(&format!("/{dir}/")) || s.contains(&format!("\\{dir}\\"))
    });
    has_ext && !is_excluded
}

fn spawn_watcher(slug: String, project_path: String, app: AppHandle) -> std::sync::mpsc::Sender<()> {
    let (stop_tx, stop_rx) = std::sync::mpsc::channel::<()>();

    std::thread::spawn(move || {
        let (tx, rx) = std::sync::mpsc::channel();
        let mut debouncer = match new_debouncer(Duration::from_millis(500), tx) {
            Ok(d) => d,
            Err(_) => return,
        };

        if debouncer
            .watcher()
            .watch(Path::new(&project_path), notify::RecursiveMode::Recursive)
            .is_err()
        {
            return;
        }

        loop {
            if stop_rx.try_recv().is_ok() {
                break;
            }

            match rx.recv_timeout(Duration::from_millis(500)) {
                Ok(Ok(events)) => {
                    let changed: Vec<PathBuf> = events.iter()
                        .filter(|e| e.kind == DebouncedEventKind::Any)
                        .map(|e| e.path.clone())
                        .filter(|p| is_relevant(p))
                        .collect::<std::collections::HashSet<_>>()
                        .into_iter()
                        .collect();

                    if !changed.is_empty() {
                        app.emit("auto-reindex-started", &slug).ok();
                        rebuild_project_incremental(&slug, &changed);
                        app.emit("indexing-complete", &slug).ok();
                    }
                }
                Ok(Err(_)) | Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => continue,
            }
        }
    });

    stop_tx
}

#[tauri::command]
pub fn start_watching(
    slug: String,
    app: AppHandle,
    state: State<Mutex<AppState>>,
) -> Result<(), String> {
    let project = Registry::get(&slug).ok_or_else(|| format!("Project {slug} not found"))?;

    let mut app_state = state.lock().unwrap();
    if app_state.watchers.contains_key(&slug) {
        return Ok(());
    }

    let stop_tx = spawn_watcher(slug.clone(), project.path.clone(), app);
    app_state.watchers.insert(slug, WatcherHandle { _stop_tx: stop_tx });
    Ok(())
}

#[tauri::command]
pub fn start_watching_all(
    app: AppHandle,
    state: State<Mutex<AppState>>,
) -> Result<usize, String> {
    let projects = Registry::list();
    let mut app_state = state.lock().unwrap();
    let mut started = 0usize;

    for project in projects {
        if app_state.watchers.contains_key(&project.slug) {
            continue;
        }
        let stop_tx = spawn_watcher(project.slug.clone(), project.path.clone(), app.clone());
        app_state.watchers.insert(project.slug, WatcherHandle { _stop_tx: stop_tx });
        started += 1;
    }

    Ok(started)
}

#[tauri::command]
pub fn stop_watching(slug: String, state: State<Mutex<AppState>>) -> Result<(), String> {
    let mut app_state = state.lock().unwrap();
    app_state.watchers.remove(&slug);
    Ok(())
}

#[tauri::command]
pub fn stop_watching_all(state: State<Mutex<AppState>>) -> Result<(), String> {
    let mut app_state = state.lock().unwrap();
    app_state.watchers.clear();
    Ok(())
}

#[tauri::command]
pub fn get_watch_status(state: State<Mutex<AppState>>) -> HashMap<String, bool> {
    let app_state = state.lock().unwrap();
    app_state.watchers.keys().map(|k| (k.clone(), true)).collect()
}

fn rebuild_project_incremental(slug: &str, changed_paths: &[PathBuf]) {
    let project = match Registry::get(slug) {
        Some(p) => p,
        None => return,
    };

    let project_path = PathBuf::from(&project.path);
    let rel_paths: Vec<String> = changed_paths
        .iter()
        .filter_map(|p| p.strip_prefix(&project_path).ok().map(|r| r.to_string_lossy().to_string()))
        .collect();

    if rel_paths.is_empty() {
        return;
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

    builder.build(&project.path, &options);

    Registry::update_project(slug, |p| {
        p.last_build = Some(chrono::Utc::now().to_rfc3339());
    });
}

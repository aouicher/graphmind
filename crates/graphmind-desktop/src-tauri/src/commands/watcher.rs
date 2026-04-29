use crate::state::{AppState, WatcherHandle};
use graphmind_config::{paths, Registry};
use graphmind_db::builder::{BuildOptions, GraphBuilder};
use notify_debouncer_mini::{new_debouncer, DebouncedEventKind};
use std::collections::HashMap;
use std::path::Path;
use std::sync::Mutex;
use std::time::Duration;
use tauri::{AppHandle, Emitter, State};

#[tauri::command]
pub fn start_watching(
    slug: String,
    app: AppHandle,
    state: State<Mutex<AppState>>,
) -> Result<(), String> {
    let project =
        Registry::get(&slug).ok_or_else(|| format!("Project {slug} not found"))?;

    let mut app_state = state.lock().unwrap();
    if app_state.watchers.contains_key(&slug) {
        return Ok(());
    }

    let (stop_tx, stop_rx) = std::sync::mpsc::channel::<()>();
    let slug_clone = slug.clone();
    let project_path = project.path.clone();

    std::thread::spawn(move || {
        let (tx, rx) = std::sync::mpsc::channel();
        let mut debouncer = match new_debouncer(Duration::from_secs(2), tx) {
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
                    let dominated_exts = [
                        ".ts", ".tsx", ".js", ".jsx", ".mjs", ".py", ".go", ".rs", ".rb",
                        ".tf", ".yml", ".yaml",
                    ];
                    let has_relevant = events.iter().any(|e| {
                        if e.kind != DebouncedEventKind::Any {
                            return false;
                        }
                        let path_str = e.path.to_string_lossy();
                        dominated_exts.iter().any(|ext| path_str.ends_with(ext))
                    });
                    if has_relevant {
                        app.emit("auto-reindex-started", &slug_clone).ok();
                        rebuild_project(&slug_clone);
                        app.emit("indexing-complete", &slug_clone).ok();
                    }
                }
                Ok(Err(_)) | Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => continue,
            }
        }
    });

    app_state
        .watchers
        .insert(slug, WatcherHandle { _stop_tx: stop_tx });
    Ok(())
}

#[tauri::command]
pub fn stop_watching(slug: String, state: State<Mutex<AppState>>) -> Result<(), String> {
    let mut app_state = state.lock().unwrap();
    app_state.watchers.remove(&slug);
    Ok(())
}

#[tauri::command]
pub fn get_watch_status(state: State<Mutex<AppState>>) -> HashMap<String, bool> {
    let app_state = state.lock().unwrap();
    app_state
        .watchers
        .keys()
        .map(|k| (k.clone(), true))
        .collect()
}

fn rebuild_project(slug: &str) {
    let project = match Registry::get(slug) {
        Some(p) => p,
        None => return,
    };

    let db_path = paths::graph_db_path(slug);
    let cache_dir = paths::cache_dir_path(slug);
    let db_path_str = db_path.to_string_lossy().to_string();
    let cache_dir_str = cache_dir.to_string_lossy().to_string();

    let mut builder = GraphBuilder::new(&db_path_str, &cache_dir_str);
    let options = BuildOptions::default();
    builder.build(&project.path, &options);

    Registry::update_project(slug, |p| {
        p.last_build = Some(chrono::Utc::now().to_rfc3339());
    });
}

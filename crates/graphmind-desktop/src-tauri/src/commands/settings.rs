use graphmind_config::{load_config, save_config, Registry};
use serde::Serialize;

#[tauri::command]
pub fn get_app_version() -> String {
    let version = env!("CARGO_PKG_VERSION");
    let hash = env!("GIT_SHORT_HASH");
    if hash.is_empty() {
        version.to_string()
    } else {
        format!("{} ({})", version, hash)
    }
}

#[derive(Serialize)]
pub struct ExcludeSettings {
    pub global: Vec<String>,
    pub project: Vec<String>,
}

#[tauri::command]
pub fn get_excludes(slug: Option<String>) -> ExcludeSettings {
    let config = load_config();
    let project = slug
        .and_then(|s| config.projects.get(&s).cloned())
        .map(|p| p.exclude)
        .unwrap_or_default();
    ExcludeSettings {
        global: config.global_exclude,
        project,
    }
}

#[tauri::command]
pub fn set_global_excludes(excludes: Vec<String>) {
    let mut config = load_config();
    config.global_exclude = excludes;
    save_config(&config);
}

#[tauri::command]
pub fn set_project_excludes(slug: String, excludes: Vec<String>) {
    Registry::update_project(&slug, |p| {
        p.exclude = excludes.clone();
    });
}

use graphmind_config::{load_config, save_config, Registry};
use serde::Serialize;
use std::path::PathBuf;

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

fn hook_path() -> PathBuf {
    dirs::home_dir()
        .expect("Could not find home directory")
        .join(".claude")
        .join("hooks")
        .join("graphmind-search.sh")
}

#[tauri::command]
pub fn get_hook_status() -> bool {
    hook_path().exists()
}

#[tauri::command]
pub fn install_claude_hook() -> Result<(), String> {
    let output = std::process::Command::new("graphmind")
        .args(["install", "hook-claude"])
        .output()
        .map_err(|e| format!("Failed to run graphmind: {}", e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("Install hook failed: {}", stderr));
    }
    Ok(())
}

#[tauri::command]
pub fn uninstall_claude_hook() -> Result<(), String> {
    let output = std::process::Command::new("graphmind")
        .args(["uninstall", "hook-claude"])
        .output()
        .map_err(|e| format!("Failed to run graphmind: {}", e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("Uninstall hook failed: {}", stderr));
    }
    Ok(())
}

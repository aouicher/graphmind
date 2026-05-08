use serde::Serialize;
use tauri::AppHandle;
use tauri_plugin_updater::UpdaterExt;

#[derive(Serialize)]
pub struct AppUpdateInfo {
    pub update_available: bool,
    pub current_version: String,
    pub new_version: Option<String>,
}

#[tauri::command]
pub async fn check_app_update(app: AppHandle) -> Result<AppUpdateInfo, String> {
    let current_version = app.package_info().version.to_string();

    let updater = app.updater().map_err(|e| format!("Updater init failed: {e}"))?;
    let update = updater.check().await.map_err(|e| format!("Update check failed: {e}"))?;

    match update {
        Some(u) => Ok(AppUpdateInfo {
            update_available: true,
            current_version,
            new_version: Some(u.version.clone()),
        }),
        None => Ok(AppUpdateInfo {
            update_available: false,
            current_version,
            new_version: None,
        }),
    }
}

#[tauri::command]
pub async fn install_app_update(app: AppHandle) -> Result<String, String> {
    let updater = app.updater().map_err(|e| format!("Updater init failed: {e}"))?;
    let update = updater.check().await.map_err(|e| format!("Update check failed: {e}"))?;

    let Some(update) = update else {
        return Err("No update available".to_string());
    };

    let new_version = update.version.clone();

    update
        .download_and_install(|_, _| {}, || {})
        .await
        .map_err(|e| format!("Install failed: {e}"))?;

    Ok(new_version)
}


/// Find graphmind binary: check known install paths first, then PATH.
pub fn find_graphmind_binary() -> String {
    // Resolve via PATH (same as the shell would) — handles all install locations
    let path_env = std::env::var("PATH").unwrap_or_default();
    let mut candidates: Vec<std::path::PathBuf> = path_env
        .split(':')
        .map(|dir| std::path::Path::new(dir).join("graphmind"))
        .filter(|p| p.exists())
        .collect();

    // Also inject known locations that the app's PATH may not include
    if let Some(home) = dirs::home_dir() {
        for extra in [
            home.join(".local").join("bin").join("graphmind"),
            home.join(".graphmind").join("bin").join("graphmind"),
        ] {
            if extra.exists() && !candidates.contains(&extra) {
                candidates.push(extra);
            }
        }
    }
    for extra in ["/usr/local/bin/graphmind", "/opt/homebrew/bin/graphmind"] {
        let p = std::path::PathBuf::from(extra);
        if p.exists() && !candidates.contains(&p) {
            candidates.push(p);
        }
    }

    candidates
        .into_iter()
        .next()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|| "graphmind".to_string())
}

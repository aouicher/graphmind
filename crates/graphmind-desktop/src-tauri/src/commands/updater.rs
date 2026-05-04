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
    // Check ~/.graphmind/bin first (primary install location)
    if let Some(home) = dirs::home_dir() {
        let local = home.join(".graphmind").join("bin").join("graphmind");
        if local.exists() {
            return local.to_string_lossy().to_string();
        }
        // Also check ~/.local/bin
        let local2 = home.join(".local").join("bin").join("graphmind");
        if local2.exists() {
            return local2.to_string_lossy().to_string();
        }
    }
    // Try /usr/local/bin directly (no shell needed)
    let usr_local = std::path::Path::new("/usr/local/bin/graphmind");
    if usr_local.exists() {
        return usr_local.to_string_lossy().to_string();
    }
    let usr_local2 = std::path::Path::new("/opt/homebrew/bin/graphmind");
    if usr_local2.exists() {
        return usr_local2.to_string_lossy().to_string();
    }
    // Last resort: rely on shell (may fail in app context)
    "graphmind".to_string()
}

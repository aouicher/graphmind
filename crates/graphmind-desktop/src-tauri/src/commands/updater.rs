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

    // Download and install the app update
    update
        .download_and_install(|_, _| {}, || {})
        .await
        .map_err(|e| format!("Install failed: {e}"))?;

    // Also update CLI if not installed via Homebrew
    update_cli_if_needed(&new_version);

    Ok(new_version)
}

fn update_cli_if_needed(version: &str) {
    let cli_path = which_graphmind();
    let Some(path) = cli_path else { return };

    if path.contains("homebrew") || path.contains("Cellar") {
        return;
    }

    let asset = if cfg!(target_arch = "aarch64") {
        "graphmind-cli-macos-arm64"
    } else if cfg!(target_os = "linux") {
        "graphmind-cli-linux-x64"
    } else {
        "graphmind-cli-macos-x64"
    };

    let url = format!(
        "https://github.com/aouicher/graphmind-dist/releases/download/v{version}/{asset}"
    );

    let tmp = std::env::temp_dir().join("graphmind-cli-update");

    let status = std::process::Command::new("curl")
        .args(["-fsSL", "-o"])
        .arg(&tmp)
        .arg(&url)
        .status();

    if status.map(|s| s.success()).unwrap_or(false) {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o755)).ok();

        let backup = format!("{path}.old");
        if std::fs::rename(&path, &backup).is_ok() {
            if std::fs::rename(&tmp, &path).is_err() {
                std::fs::rename(&backup, &path).ok();
            } else {
                std::fs::remove_file(&backup).ok();
            }
        }
    }
}

fn which_graphmind() -> Option<String> {
    let output = std::process::Command::new("which")
        .arg("graphmind")
        .output()
        .ok()?;
    if output.status.success() {
        let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if !path.is_empty() {
            return Some(path);
        }
    }
    None
}

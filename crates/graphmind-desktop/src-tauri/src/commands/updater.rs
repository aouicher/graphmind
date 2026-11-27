use serde::Serialize;
use std::path::{Path, PathBuf};
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


/// The graphmind binary the app runs: the first one on PATH, then in the install locations the
/// app's PATH may not include (an app started from the Finder gets a minimal PATH). `None` when
/// there is none.
pub fn find_graphmind_binary() -> Option<PathBuf> {
    let path_env = std::env::var("PATH").unwrap_or_default();
    first_graphmind_in(&search_dirs(&path_env, dirs::home_dir().as_deref()))
}

/// `find_graphmind_binary` for commands that run graphmind, with the error they report when
/// there is none.
pub fn require_graphmind_binary() -> Result<PathBuf, String> {
    find_graphmind_binary().ok_or_else(|| {
        "graphmind CLI not found: install it with Update CLI in Settings".to_string()
    })
}

/// The folders `find_graphmind_binary` looks in, in order.
fn search_dirs(path_env: &str, home: Option<&Path>) -> Vec<PathBuf> {
    // An empty PATH entry means the working directory, which is no install location.
    let mut dirs: Vec<PathBuf> = path_env
        .split(':')
        .filter(|dir| !dir.is_empty())
        .map(PathBuf::from)
        .collect();
    if let Some(home) = home {
        dirs.push(home.join(".local").join("bin"));
        dirs.push(home.join(".graphmind").join("bin"));
    }
    dirs.extend(["/usr/local/bin", "/opt/homebrew/bin"].map(PathBuf::from));
    dirs
}

fn first_graphmind_in(dirs: &[PathBuf]) -> Option<PathBuf> {
    dirs.iter().map(|dir| dir.join("graphmind")).find(|p| p.exists())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn search_dirs_lists_path_then_install_locations() {
        let home = Path::new("/home/me");
        let expected: Vec<PathBuf> = [
            "/a",
            "/b",
            "/home/me/.local/bin",
            "/home/me/.graphmind/bin",
            "/usr/local/bin",
            "/opt/homebrew/bin",
        ]
        .map(PathBuf::from)
        .into();
        assert_eq!(search_dirs("/a::/b:", Some(home)), expected);
    }

    #[test]
    fn first_graphmind_in_returns_the_first_one_found() {
        let dir = tempfile::tempdir().unwrap();
        let [empty, first, second] = ["empty", "first", "second"].map(|d| dir.path().join(d));
        for d in [&empty, &first, &second] {
            std::fs::create_dir(d).unwrap();
        }
        for d in [&first, &second] {
            std::fs::write(d.join("graphmind"), "").unwrap();
        }

        assert_eq!(
            first_graphmind_in(&[empty, first.clone(), second]),
            Some(first.join("graphmind"))
        );
    }

    #[test]
    fn first_graphmind_in_returns_none_when_there_is_none() {
        let dir = tempfile::tempdir().unwrap();
        let missing = dir.path().join("missing");

        assert_eq!(first_graphmind_in(&[dir.path().to_path_buf(), missing]), None);
    }
}

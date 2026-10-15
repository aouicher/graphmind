use crate::types::CliStatus;
use graphmind_config::paths::home_dir;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;

#[tauri::command]
pub fn check_cli_installed() -> CliStatus {
    if let Some(path) = super::updater::find_graphmind_binary() {
        let version = get_version(&path);
        return CliStatus {
            installed: true,
            path: Some(path.to_string_lossy().into_owned()),
            version,
        };
    }

    CliStatus {
        installed: false,
        path: None,
        version: None,
    }
}

/// Installs graphmind when the app finds none, to `~/.local/bin/graphmind`, where the install
/// script puts it. A graphmind the app finds is returned as it is: replacing it could take a
/// Homebrew install from brew, or fail on a root-owned one, and updates go through
/// `graphmind update` (`update_cli`).
#[tauri::command]
pub async fn install_cli() -> Result<CliStatus, String> {
    let found = check_cli_installed();
    if found.installed {
        return Ok(found);
    }

    let bin_path = home_dir().join(".local").join("bin").join("graphmind");
    if let Some(parent) = bin_path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }

    let asset = if cfg!(target_arch = "aarch64") {
        "graphmind-cli-macos-arm64"
    } else {
        "graphmind-cli-macos-x64"
    };

    let url = format!(
        "https://github.com/aouicher/graphmind/releases/latest/download/{asset}"
    );

    download_cli(&url, &bin_path)?;

    // Re-sign on macOS to avoid Gatekeeper issues
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("codesign")
            .args(["-s", "-"])
            .arg(&bin_path)
            .output()
            .ok();
    }

    // Run setup to install hooks, skills, CLAUDE.md, MCP configs
    std::process::Command::new(&bin_path)
        .arg("setup")
        .status()
        .ok();

    let version = get_version(&bin_path);
    Ok(CliStatus {
        installed: true,
        path: Some(bin_path.to_string_lossy().to_string()),
        version,
    })
}

/// Downloads `url` next to `bin_path`, checks that it runs, then moves it into place. The
/// download is removed on every failure, so no partial binary is left in a folder on PATH.
fn download_cli(url: &str, bin_path: &Path) -> Result<(), String> {
    let tmp_path = bin_path.with_extension("tmp");
    let result = (|| {
        let status = std::process::Command::new("curl")
            .args(["-fsSL", "-o"])
            .arg(&tmp_path)
            .arg(url)
            .status()
            .map_err(|e| format!("Download failed: {e}"))?;
        if !status.success() {
            return Err("Failed to download CLI binary".to_string());
        }

        fs::set_permissions(&tmp_path, fs::Permissions::from_mode(0o755))
            .map_err(|e| e.to_string())?;

        // Verify the downloaded binary works before replacing
        let verify = std::process::Command::new(&tmp_path).arg("--version").output();
        if !verify.map(|o| o.status.success()).unwrap_or(false) {
            return Err("Downloaded binary failed verification".to_string());
        }

        fs::rename(&tmp_path, bin_path).map_err(|e| e.to_string())
    })();
    if result.is_err() {
        fs::remove_file(&tmp_path).ok();
    }
    result
}

#[tauri::command]
pub fn get_cli_path() -> Result<String, String> {
    super::updater::require_graphmind_binary().map(|path| path.to_string_lossy().into_owned())
}

#[derive(serde::Serialize)]
pub struct UpdateInfo {
    pub current: String,
    pub latest: String,
    pub update_available: bool,
}

#[tauri::command]
pub async fn check_cli_update() -> Result<UpdateInfo, String> {
    let current = super::updater::find_graphmind_binary()
        .and_then(|path| get_version(&path))
        .unwrap_or_else(|| "0.0.0".to_string());

    let output = std::process::Command::new("curl")
        .args([
            "-fsSL",
            "-H", "Accept: application/vnd.github+json",
            "-H", "User-Agent: graphmind-desktop",
            "https://api.github.com/repos/aouicher/graphmind/releases/latest",
        ])
        .output()
        .map_err(|e| format!("Failed to check for updates: {e}"))?;

    if !output.status.success() {
        return Err("Failed to fetch latest release".to_string());
    }

    let body = String::from_utf8_lossy(&output.stdout);
    let latest = body
        .split("\"tag_name\"")
        .nth(1)
        .and_then(|s| s.split('"').nth(1))
        .map(|s| s.trim_start_matches('v').to_string())
        .unwrap_or_else(|| current.clone());

    let update_available = latest != current;

    Ok(UpdateInfo {
        current,
        latest,
        update_available,
    })
}

#[tauri::command]
pub async fn update_cli() -> Result<CliStatus, String> {
    let Some(cli_path) = super::updater::find_graphmind_binary() else {
        // CLI not installed yet — do a fresh install
        return install_cli().await;
    };

    // Delegate to the CLI's own update logic (handles Homebrew symlinks correctly)
    let status = std::process::Command::new(&cli_path)
        .arg("update")
        .status()
        .map_err(|e| format!("Failed to run graphmind update: {e}"))?;

    if !status.success() {
        return Err("graphmind update failed".to_string());
    }

    // Return updated CLI status
    let version = get_version(&cli_path);
    Ok(CliStatus {
        installed: true,
        path: Some(cli_path.to_string_lossy().into_owned()),
        version,
    })
}

fn get_version(path: &Path) -> Option<String> {
    std::process::Command::new(path)
        .arg("--version")
        .output()
        .ok()
        .and_then(|o| {
            if o.status.success() {
                let v = String::from_utf8_lossy(&o.stdout).trim().to_string();
                Some(v.replace("graphmind ", ""))
            } else {
                None
            }
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A stand-in for the release asset in `dir`, as a `file://` URL: a script that answers
    /// `--version` like graphmind.
    fn fake_release(dir: &Path) -> String {
        let release = dir.join("release");
        fs::write(&release, "#!/bin/sh\necho graphmind 9.9.9\n").unwrap();
        format!("file://{}", release.display())
    }

    #[test]
    fn download_cli_installs_a_binary_that_runs() {
        let dir = tempfile::tempdir().unwrap();
        let bin = dir.path().join("graphmind");

        download_cli(&fake_release(dir.path()), &bin).unwrap();

        assert_eq!(get_version(&bin).as_deref(), Some("9.9.9"));
        assert!(!bin.with_extension("tmp").exists());
    }

    /// Every failure removes the download: a download that fails, a binary that doesn't run, and
    /// a move into place that fails (a folder where the binary goes).
    #[test]
    fn download_cli_leaves_no_temp_file_when_it_fails() {
        let dir = tempfile::tempdir().unwrap();
        let bin = dir.path().join("graphmind");
        let tmp = bin.with_extension("tmp");

        let missing = format!("file://{}", dir.path().join("missing").display());
        assert!(download_cli(&missing, &bin).is_err());
        assert!(!tmp.exists(), "after a failed download");

        let not_a_binary = dir.path().join("not-a-binary");
        fs::write(&not_a_binary, "not a binary").unwrap();
        assert!(download_cli(&format!("file://{}", not_a_binary.display()), &bin).is_err());
        assert!(!tmp.exists(), "after a failed verification");

        fs::create_dir_all(bin.join("occupied")).unwrap();
        assert!(download_cli(&fake_release(dir.path()), &bin).is_err());
        assert!(!tmp.exists(), "after a failed rename");
    }
}

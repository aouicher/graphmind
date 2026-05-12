use crate::types::CliStatus;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;

fn home_dir() -> PathBuf {
    dirs::home_dir().expect("Cannot determine home directory")
}

fn local_bin_path() -> PathBuf {
    home_dir().join(".graphmind").join("bin").join("graphmind")
}

#[tauri::command]
pub fn check_cli_installed() -> CliStatus {
    let path = super::updater::find_graphmind_binary();
    if path != "graphmind" {
        let version = get_version(&path);
        return CliStatus {
            installed: true,
            path: Some(path),
            version,
        };
    }

    CliStatus {
        installed: false,
        path: None,
        version: None,
    }
}

#[tauri::command]
pub async fn install_cli() -> Result<CliStatus, String> {
    // Install to existing location, or fall back to ~/.graphmind/bin
    let existing = super::updater::find_graphmind_binary();
    let bin_path = if existing != "graphmind" {
        std::path::PathBuf::from(&existing)
    } else {
        let bin_dir = home_dir().join(".graphmind").join("bin");
        fs::create_dir_all(&bin_dir).map_err(|e| e.to_string())?;
        bin_dir.join("graphmind")
    };

    // Ensure parent dir exists
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

    let tmp_path = bin_path.with_extension("tmp");

    let status = std::process::Command::new("curl")
        .args(["-fsSL", "-o"])
        .arg(&tmp_path)
        .arg(&url)
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
        fs::remove_file(&tmp_path).ok();
        return Err("Downloaded binary failed verification".to_string());
    }

    fs::rename(&tmp_path, &bin_path).map_err(|e| e.to_string())?;

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

    let version = get_version(&bin_path.to_string_lossy());
    Ok(CliStatus {
        installed: true,
        path: Some(bin_path.to_string_lossy().to_string()),
        version,
    })
}

#[tauri::command]
pub fn get_cli_path() -> String {
    super::updater::find_graphmind_binary()
}

#[derive(serde::Serialize)]
pub struct UpdateInfo {
    pub current: String,
    pub latest: String,
    pub update_available: bool,
}

#[tauri::command]
pub async fn check_cli_update() -> Result<UpdateInfo, String> {
    let cli_path = super::updater::find_graphmind_binary();
    let current = get_version(&cli_path).unwrap_or_else(|| "0.0.0".to_string());

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
    let cli_path = super::updater::find_graphmind_binary();
    if cli_path == "graphmind" {
        // CLI not installed yet — do a fresh install
        return install_cli().await;
    }

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
        path: Some(cli_path),
        version,
    })
}

#[tauri::command]
pub fn ensure_cli_in_path() -> Result<String, String> {
    let path = super::updater::find_graphmind_binary();
    if path != "graphmind" {
        return Ok(path);
    }

    let local = local_bin_path();
    if !local.exists() {
        return Err("CLI not installed yet".to_string());
    }

    // Try symlink to /usr/local/bin (no sudo needed on macOS if dir exists)
    let target = PathBuf::from("/usr/local/bin/graphmind");
    if !target.exists() && std::os::unix::fs::symlink(&local, &target).is_err() {
        // Fallback: add ~/.graphmind/bin to shell profiles
        let bin_dir = home_dir().join(".graphmind").join("bin");
        let line = format!("\nexport PATH=\"{}:$PATH\"\n", bin_dir.display());
        for profile in &[".zshrc", ".bashrc", ".bash_profile", ".profile"] {
            let path = home_dir().join(profile);
            if path.exists() {
                let content = fs::read_to_string(&path).unwrap_or_default();
                if !content.contains(".graphmind/bin") {
                    fs::write(&path, format!("{}{}", content, line)).ok();
                }
            }
        }
        let current_path = std::env::var("PATH").unwrap_or_default();
        std::env::set_var(
            "PATH",
            format!("{}:{}", bin_dir.display(), current_path),
        );
        return Ok(local.to_string_lossy().to_string());
    }

    Ok(target.to_string_lossy().to_string())
}

fn get_version(path: &str) -> Option<String> {
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

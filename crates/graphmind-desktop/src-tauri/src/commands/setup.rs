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
    // Check via which
    if let Ok(output) = std::process::Command::new("which")
        .arg("graphmind")
        .output()
    {
        if output.status.success() {
            let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
            let version = get_version(&path);
            return CliStatus {
                installed: true,
                path: Some(path),
                version,
            };
        }
    }

    // Check local install
    let local = local_bin_path();
    if local.exists() {
        let version = get_version(&local.to_string_lossy());
        return CliStatus {
            installed: true,
            path: Some(local.to_string_lossy().to_string()),
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
    let bin_dir = home_dir().join(".graphmind").join("bin");
    fs::create_dir_all(&bin_dir).map_err(|e| e.to_string())?;

    let asset = if cfg!(target_arch = "aarch64") {
        "graphmind-cli-macos-arm64"
    } else {
        "graphmind-cli-macos-x64"
    };

    let url = format!(
        "https://github.com/aouicher/graphmind-dist/releases/latest/download/{asset}"
    );

    let bin_path = bin_dir.join("graphmind");

    let status = std::process::Command::new("curl")
        .args(["-fsSL", "-o"])
        .arg(&bin_path)
        .arg(&url)
        .status()
        .map_err(|e| format!("Download failed: {e}"))?;

    if !status.success() {
        return Err("Failed to download CLI binary".to_string());
    }
    fs::set_permissions(&bin_path, fs::Permissions::from_mode(0o755))
        .map_err(|e| e.to_string())?;

    let version = get_version(&bin_path.to_string_lossy());
    Ok(CliStatus {
        installed: true,
        path: Some(bin_path.to_string_lossy().to_string()),
        version,
    })
}

#[tauri::command]
pub fn get_cli_path() -> String {
    if let Ok(output) = std::process::Command::new("which")
        .arg("graphmind")
        .output()
    {
        if output.status.success() {
            return String::from_utf8_lossy(&output.stdout).trim().to_string();
        }
    }
    let local = local_bin_path();
    if local.exists() {
        return local.to_string_lossy().to_string();
    }
    "graphmind".to_string()
}

#[derive(serde::Serialize)]
pub struct UpdateInfo {
    pub current: String,
    pub latest: String,
    pub update_available: bool,
}

#[tauri::command]
pub async fn check_cli_update() -> Result<UpdateInfo, String> {
    let cli_path = get_cli_path();

    let current = get_version(&cli_path).unwrap_or_else(|| "0.0.0".to_string());

    let output = std::process::Command::new("curl")
        .args([
            "-fsSL",
            "-H",
            "Accept: application/vnd.github+json",
            "https://api.github.com/repos/aouicher/graphmind-dist/releases/latest",
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
    install_cli().await
}

#[tauri::command]
pub fn ensure_cli_in_path() -> Result<String, String> {
    // If already in PATH, nothing to do
    if let Ok(output) = std::process::Command::new("which")
        .arg("graphmind")
        .output()
    {
        if output.status.success() {
            let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if !path.is_empty() {
                return Ok(path);
            }
        }
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
        // Also export for current process
        let current_path = std::env::var("PATH").unwrap_or_default();
        std::env::set_var("PATH", format!("{}:{}", bin_dir.display(), current_path));
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

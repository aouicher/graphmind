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

    let target = if cfg!(target_arch = "aarch64") {
        "aarch64-apple-darwin"
    } else {
        "x86_64-apple-darwin"
    };

    let url = format!(
        "https://github.com/aouicher/graphmind/releases/latest/download/graphmind-{target}.tar.gz"
    );

    let output = std::process::Command::new("curl")
        .args(["-fsSL", &url])
        .output()
        .map_err(|e| format!("Download failed: {e}"))?;

    if !output.status.success() {
        return Err("Failed to download CLI binary".to_string());
    }

    let tar_output = std::process::Command::new("tar")
        .args(["xzf", "-", "-C"])
        .arg(&bin_dir)
        .stdin(std::process::Stdio::piped())
        .spawn()
        .and_then(|mut child| {
            use std::io::Write;
            child.stdin.as_mut().unwrap().write_all(&output.stdout)?;
            child.wait()
        })
        .map_err(|e| format!("Extract failed: {e}"))?;

    if !tar_output.success() {
        return Err("Failed to extract CLI binary".to_string());
    }

    let bin_path = bin_dir.join("graphmind");
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

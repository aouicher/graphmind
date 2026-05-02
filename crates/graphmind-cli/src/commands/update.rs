use colored::Colorize;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::process::Command;

const GITHUB_REPO: &str = "aouicher/graphmind-dist";

fn current_version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

fn get_latest_version() -> Result<String, String> {
    let output = Command::new("curl")
        .args([
            "-fsSL",
            "-H",
            "Accept: application/vnd.github+json",
            &format!("https://api.github.com/repos/{GITHUB_REPO}/releases/latest"),
        ])
        .output()
        .map_err(|e| format!("Failed to check for updates: {e}"))?;

    if !output.status.success() {
        return Err("Failed to fetch latest release info".to_string());
    }

    let body = String::from_utf8_lossy(&output.stdout);
    // Extract tag_name from JSON (avoid serde dependency)
    let tag = body
        .split("\"tag_name\"")
        .nth(1)
        .and_then(|s| s.split('"').nth(1))
        .ok_or_else(|| "Could not parse release tag".to_string())?;

    Ok(tag.trim_start_matches('v').to_string())
}

fn get_current_binary_path() -> PathBuf {
    std::env::current_exe().unwrap_or_else(|_| PathBuf::from("graphmind"))
}

fn download_and_replace(version: &str) -> Result<(), String> {
    let asset = if cfg!(target_arch = "aarch64") {
        "graphmind-cli-macos-arm64"
    } else if cfg!(target_os = "linux") {
        "graphmind-cli-linux-x64"
    } else {
        "graphmind-cli-macos-x64"
    };

    let url = format!(
        "https://github.com/{GITHUB_REPO}/releases/download/v{version}/{asset}"
    );

    println!("  {} v{version} ({asset})...", "Downloading".blue());

    let tmp_path = std::env::temp_dir().join("graphmind-update-bin");

    let status = Command::new("curl")
        .args(["-fsSL", "-o"])
        .arg(&tmp_path)
        .arg(&url)
        .status()
        .map_err(|e| format!("Download failed: {e}"))?;

    if !status.success() {
        return Err(format!("Failed to download v{version} from {url}"));
    }

    fs::set_permissions(&tmp_path, fs::Permissions::from_mode(0o755))
        .map_err(|e| e.to_string())?;

    let bin_path = get_current_binary_path();
    let backup = bin_path.with_extension("old");
    fs::rename(&bin_path, &backup).map_err(|e| format!("Failed to backup current binary: {e}"))?;

    if let Err(e) = fs::rename(&tmp_path, &bin_path) {
        fs::rename(&backup, &bin_path).ok();
        return Err(format!("Failed to install new binary: {e}"));
    }

    fs::remove_file(&backup).ok();

    Ok(())
}

pub fn update(check_only: bool) {
    let current = current_version();

    print!("  {} ", "Checking".blue());
    println!("current version: v{current}");

    let latest = match get_latest_version() {
        Ok(v) => v,
        Err(e) => {
            eprintln!("  {} {e}", "Error:".red());
            std::process::exit(1);
        }
    };

    if latest == current {
        println!("  {} already on latest (v{current})", "✓".green());
        return;
    }

    println!("  {} v{current} → v{latest}", "Update available:".yellow());

    if check_only {
        println!("\n  Run {} to update.", "graphmind update".bold());
        return;
    }

    // Check if installed via Homebrew
    let bin_path = get_current_binary_path();
    let bin_str = bin_path.to_string_lossy();
    if bin_str.contains("homebrew") || bin_str.contains("Cellar") {
        println!(
            "  {} Installed via Homebrew. Run: {}",
            "Note:".yellow(),
            "brew upgrade graphmind".bold()
        );
        return;
    }

    match download_and_replace(&latest) {
        Ok(()) => {
            println!("  {} Updated to v{latest}", "✓".green());
        }
        Err(e) => {
            eprintln!("  {} {e}", "Error:".red());
            std::process::exit(1);
        }
    }
}

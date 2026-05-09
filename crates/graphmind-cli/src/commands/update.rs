use colored::Colorize;
use graphmind_config::update_crosses_breaking;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::process::Command;

const GITHUB_REPO: &str = "aouicher/graphmind-dist";

fn current_version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

fn get_latest_version() -> Result<String, String> {
    let use_rtk = which::which("rtk").is_ok();
    let url = format!("https://api.github.com/repos/{GITHUB_REPO}/releases/latest");
    let output = if use_rtk {
        Command::new("rtk")
            .args(["proxy", "curl", "-fsSL", "--max-time", "10",
                   "-H", "Accept: application/vnd.github+json", &url])
            .output()
    } else {
        Command::new("curl")
            .args(["-fsSL", "--max-time", "10",
                   "-H", "Accept: application/vnd.github+json", &url])
            .output()
    }.map_err(|e| format!("Failed to check for updates: {e}"))?;

    if !output.status.success() {
        return Err("Failed to fetch latest release info".to_string());
    }

    let body = String::from_utf8_lossy(&output.stdout);
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

    // Verify downloaded binary is valid
    fs::set_permissions(&tmp_path, fs::Permissions::from_mode(0o755))
        .map_err(|e| format!("Failed to set permissions: {e}"))?;

    let verify = Command::new(&tmp_path)
        .arg("--version")
        .output();

    match verify {
        Ok(output) if output.status.success() => {
            let out = String::from_utf8_lossy(&output.stdout);
            if !out.contains(version) {
                return Err(format!(
                    "Downloaded binary version mismatch (expected v{version}, got: {})",
                    out.trim()
                ));
            }
        }
        Ok(output) => {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(format!("Downloaded binary failed verification: {}", stderr.trim()));
        }
        Err(e) => {
            return Err(format!("Cannot execute downloaded binary: {e}"));
        }
    }

    println!("  {} Binary verified", "✓".green());

    let bin_path = get_current_binary_path();
    let backup = bin_path.with_extension("old");
    fs::rename(&bin_path, &backup).map_err(|e| format!("Failed to backup current binary: {e}"))?;

    if let Err(e) = fs::rename(&tmp_path, &bin_path) {
        // Restore backup on failure
        fs::rename(&backup, &bin_path).ok();
        return Err(format!("Failed to install new binary: {e}"));
    }

    // Sign on macOS to prevent Gatekeeper kill
    #[cfg(target_os = "macos")]
    {
        Command::new("codesign")
            .args(["-s", "-"])
            .arg(&bin_path)
            .output()
            .ok();
    }

    fs::remove_file(&backup).ok();

    // Remove stale copies in other known locations (e.g. ~/.local/bin from old install.sh default)
    if let Some(home) = dirs::home_dir() {
        let stale_locations = [
            home.join(".local").join("bin").join("graphmind"),
        ];
        for stale in &stale_locations {
            if stale != &bin_path && stale.exists() {
                fs::remove_file(stale).ok();
            }
        }
    }

    Ok(())
}


pub fn update(check_only: bool) {
    let current = current_version();

    println!("  {} v{current}", "Current:".dimmed());

    let latest = match get_latest_version() {
        Ok(v) => v,
        Err(e) => {
            eprintln!("  {} {e}", "Error:".red());
            eprintln!("  Check your network or try again later.");
            std::process::exit(1);
        }
    };

    if latest == current {
        println!("  {} Already on latest (v{current})", "✓".green().bold());
        return;
    }

    println!("  {} v{current} → v{latest}", "Update available:".yellow().bold());

    if check_only {
        println!("\n  Run {} to install.", "graphmind update".bold());
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
            println!("  {} Updated to v{latest}", "✓".green().bold());
            println!();
            println!("  {} Refreshing hooks and skills...", ">>".cyan().bold());
            super::setup::setup();
            // Check via version crossing OR stale DB schema
            let schema_stale = graphmind_config::Registry::list().iter().any(|p| {
                let db_path = graphmind_config::paths::graph_db_path(&p.slug);
                graphmind_db::schema::schema_needs_reset(db_path.to_str().unwrap_or(""))
            });
            if update_crosses_breaking(current, &latest) || schema_stale {
                println!();
                println!(
                    "  {} This update changed the graph schema.",
                    "Important:".yellow().bold()
                );
                println!(
                    "  Run {} to reindex all projects with the new edge kinds.",
                    "graphmind build --reset --all".bold()
                );
            }
        }
        Err(e) => {
            eprintln!("  {} {e}", "Error:".red().bold());
            eprintln!("  Your current version (v{current}) is unchanged.");
            std::process::exit(1);
        }
    }
}

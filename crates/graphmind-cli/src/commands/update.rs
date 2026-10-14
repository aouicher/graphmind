use colored::Colorize;
use graphmind_config::update_crosses_breaking;
use std::fs;
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;

const GITHUB_REPO: &str = "aouicher/graphmind";

fn current_version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

fn get_latest_version() -> Result<String, String> {
    let url = format!("https://api.github.com/repos/{GITHUB_REPO}/releases/latest");
    let output = graphmind_config::rtk::curl(&["-fsSL", "--max-time", "10",
                                               "-H", "Accept: application/vnd.github+json", &url])
        .map_err(|e| format!("Failed to check for updates: {e}"))?;

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
    // Match on OS first: `target_arch` alone sent Windows and Linux arm64
    // hosts to the macOS binary (issue #109).
    let asset = if cfg!(target_os = "windows") {
        "graphmind-cli-windows-x64.exe"
    } else if cfg!(target_os = "linux") {
        "graphmind-cli-linux-x64"
    } else if cfg!(target_os = "macos") {
        if cfg!(target_arch = "aarch64") {
            "graphmind-cli-macos-arm64"
        } else {
            "graphmind-cli-macos-x64"
        }
    } else {
        return Err(format!(
            "No prebuilt binary for this platform ({}). Build from source: cargo install --git https://github.com/{GITHUB_REPO}",
            std::env::consts::OS
        ));
    };

    let url = format!(
        "https://github.com/{GITHUB_REPO}/releases/download/v{version}/{asset}"
    );

    println!("  {} v{version} ({asset})...", "Downloading".blue());

    let bin_path = get_current_binary_path();
    replace_binary(&bin_path, version, |tmp_path| {
        let status = Command::new("curl")
            .args(["-fsSL", "-o"])
            .arg(tmp_path)
            .arg(&url)
            .status()
            .map_err(|e| format!("Download failed: {e}"))?;
        if status.success() {
            Ok(())
        } else {
            Err(format!("Failed to download v{version} from {url}"))
        }
    })?;

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


/// Replace the binary at `bin_path` with the one `fetch` writes to the path it is given, after
/// checking that it runs and reports `version`.
///
/// The download goes to a file with a unique name, created for this run next to the binary: two
/// updates at once don't share it, no other user can put a file there first, and the final
/// rename stays on one volume.
fn replace_binary(bin_path: &Path, version: &str, fetch: impl FnOnce(&Path) -> Result<(), String>) -> Result<(), String> {
    let dir = bin_path.parent().filter(|d| !d.as_os_str().is_empty()).unwrap_or(Path::new("."));
    // Windows runs a file only with the .exe extension, which the check below needs.
    let tmp_path = tempfile::Builder::new()
        .prefix(".graphmind-update-")
        .suffix(std::env::consts::EXE_SUFFIX)
        .tempfile_in(dir)
        .map_err(|e| format!("Cannot write to {}: {e}", dir.display()))?
        .into_temp_path();

    fetch(&tmp_path)?;

    #[cfg(unix)]
    fs::set_permissions(&tmp_path, fs::Permissions::from_mode(0o755))
        .map_err(|e| format!("Failed to set permissions: {e}"))?;

    let output = Command::new(&*tmp_path)
        .arg("--version")
        .output()
        .map_err(|e| format!("Cannot execute downloaded binary: {e}"))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let reason = if stderr.trim().is_empty() { output.status.to_string() } else { stderr.trim().to_string() };
        return Err(format!("Downloaded binary failed verification: {reason}"));
    }
    let out = String::from_utf8_lossy(&output.stdout);
    if !out.contains(version) {
        return Err(format!(
            "Downloaded binary version mismatch (expected v{version}, got: {})",
            out.trim()
        ));
    }

    println!("  {} Binary verified", "✓".green());

    // Append rather than replace the extension: `with_extension` would turn
    // `graphmind.exe` into `graphmind.old`, losing the `.exe` on rollback.
    let mut backup = bin_path.as_os_str().to_owned();
    backup.push(".old");
    let backup = PathBuf::from(backup);
    // Windows refuses to rename onto an existing path, and leaves this behind
    // because the running image stays locked until the process exits.
    fs::remove_file(&backup).ok();
    fs::rename(bin_path, &backup).map_err(|e| format!("Failed to backup current binary: {e}"))?;

    if let Err(e) = tmp_path.persist(bin_path) {
        // Restore backup on failure
        fs::rename(&backup, bin_path).ok();
        return Err(format!("Failed to install new binary: {}", e.error));
    }

    // Sign on macOS to prevent Gatekeeper kill
    #[cfg(target_os = "macos")]
    {
        Command::new("codesign")
            .args(["-s", "-"])
            .arg(bin_path)
            .output()
            .ok();
    }

    fs::remove_file(&backup).ok();
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

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::cell::RefCell;

    /// A directory holding a fake installed `graphmind` 1.0.0.
    fn installed() -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let bin = dir.path().join("graphmind");
        fs::write(&bin, "#!/bin/sh\necho 'graphmind 1.0.0'\n").unwrap();
        fs::set_permissions(&bin, fs::Permissions::from_mode(0o755)).unwrap();
        (dir, bin)
    }

    /// A fetch that writes `script` as the downloaded binary.
    fn download(script: &'static str) -> impl FnOnce(&Path) -> Result<(), String> {
        move |path: &Path| fs::write(path, script).map_err(|e| e.to_string())
    }

    fn entries(dir: &Path) -> Vec<String> {
        let mut names: Vec<String> = fs::read_dir(dir).unwrap().map(|e| e.unwrap().file_name().to_string_lossy().into_owned()).collect();
        names.sort();
        names
    }

    #[test]
    fn the_download_goes_to_a_unique_file_next_to_the_binary_and_replaces_it() {
        let (dir, bin) = installed();
        let seen = RefCell::new(None);
        replace_binary(&bin, "9.9.9", |path| {
            *seen.borrow_mut() = Some(path.to_path_buf());
            download("#!/bin/sh\necho 'graphmind 9.9.9'\n")(path)
        })
        .unwrap();

        let tmp = seen.into_inner().unwrap();
        assert_eq!(tmp.parent(), Some(dir.path()), "{tmp:?}");
        assert!(tmp.file_name().unwrap().to_string_lossy().starts_with(".graphmind-update-"), "{tmp:?}");
        assert!(fs::read_to_string(&bin).unwrap().contains("9.9.9"));
        assert_eq!(entries(dir.path()), ["graphmind"], "no download or backup left behind");
    }

    #[test]
    fn two_updates_download_to_different_files() {
        let (dir, bin) = installed();
        let (first, second) = (RefCell::new(None), RefCell::new(None));
        replace_binary(&bin, "9.9.9", |path| {
            *first.borrow_mut() = Some(path.to_path_buf());
            // While this download's file exists, a second update gets another one.
            let other = tempfile::Builder::new().prefix(".graphmind-update-").tempfile_in(dir.path()).unwrap();
            *second.borrow_mut() = Some(other.path().to_path_buf());
            download("#!/bin/sh\necho 'graphmind 9.9.9'\n")(path)
        })
        .unwrap();
        assert_ne!(first.into_inner(), second.into_inner());
    }

    #[test]
    fn a_binary_killed_by_a_signal_is_reported_and_the_installed_one_is_kept() {
        let (dir, bin) = installed();
        let err = replace_binary(&bin, "9.9.9", download("#!/bin/sh\nkill -9 $$\n")).unwrap_err();

        assert!(err.contains("signal"), "{err}");
        assert!(fs::read_to_string(&bin).unwrap().contains("1.0.0"));
        assert_eq!(entries(dir.path()), ["graphmind"], "the failed download is removed");
    }

    #[test]
    fn a_binary_reporting_another_version_is_refused() {
        let (dir, bin) = installed();
        let err = replace_binary(&bin, "9.9.9", download("#!/bin/sh\necho 'graphmind 9.9.8'\n")).unwrap_err();

        assert!(err.contains("version mismatch"), "{err}");
        assert!(fs::read_to_string(&bin).unwrap().contains("1.0.0"));
        assert_eq!(entries(dir.path()), ["graphmind"]);
    }

    #[test]
    fn a_failed_download_leaves_nothing_behind() {
        let (dir, bin) = installed();
        let err = replace_binary(&bin, "9.9.9", |_| Err("Failed to download".into())).unwrap_err();

        assert_eq!(err, "Failed to download");
        assert_eq!(entries(dir.path()), ["graphmind"]);
    }
}

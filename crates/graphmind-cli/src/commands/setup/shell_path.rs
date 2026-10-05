//! Shell PATH configuration for `graphmind setup` and `uninstall`: make the installed binary
//! reachable, once, only when it isn't already, and remove exactly the lines setup added.

use super::home_dir;
use colored::Colorize;
use std::fs;
use std::path::{Path, PathBuf};

#[doc(hidden)]
pub fn install_shell_path() {
    if cfg!(target_os = "windows") {
        install_shell_path_windows();
    } else {
        install_shell_path_unix();
    }
}

/// Marks PATH lines written by setup, so uninstall removes exactly what setup added.
const PATH_MARKER: &str = "# added by graphmind setup";

/// The PATH line graphmind <= 0.2.215 wrote, for `~/.graphmind/bin`, to .zshenv, .zshrc and
/// .bashrc — whether or not the binary was installed there.
fn legacy_export_line() -> String {
    let dir = home_dir().join(".graphmind").join("bin");
    format!("export PATH=\"{}:$PATH\"", dir.to_string_lossy())
}

/// The directory setup puts on PATH for `exe`: `~/.graphmind/bin` when the binary is there
/// (install script), otherwise the directory of the executable (Homebrew, `~/.local/bin`, cargo).
fn bin_dir_for(exe: &Path) -> PathBuf {
    let managed = home_dir().join(".graphmind").join("bin");
    if managed.join("graphmind").exists() {
        return managed;
    }
    exe.parent().map(Path::to_path_buf).unwrap_or(managed)
}

/// Where PATH already reaches this graphmind, if it does:
/// - a `graphmind` on PATH that is the same file as `exe` (symlinks resolved). A symlinked
///   install (Linuxbrew, a linked dev build) runs from a versioned directory
///   (`.../Cellar/graphmind/<ver>/bin`, `target/release`) that isn't on PATH, while the link is;
/// - or the directory setup would add (`bin_dir_for`) is on PATH already.
#[doc(hidden)]
pub fn already_on_path(exe: &Path, path_env: &str) -> Option<PathBuf> {
    let dirs = || path_env.split(':').filter(|d| !d.is_empty());
    if let Ok(exe) = fs::canonicalize(exe) {
        let same_file = dirs()
            .map(|d| Path::new(d).join("graphmind"))
            .find(|candidate| fs::canonicalize(candidate).is_ok_and(|c| c == exe));
        if same_file.is_some() {
            return same_file;
        }
    }
    let bin_dir = bin_dir_for(exe);
    let want = fs::canonicalize(&bin_dir).unwrap_or_else(|_| bin_dir.clone());
    dirs()
        .any(|d| fs::canonicalize(d).unwrap_or_else(|_| d.into()) == want)
        .then_some(bin_dir)
}

fn install_shell_path_unix() {
    let Ok(exe) = std::env::current_exe() else { return };
    let path_env = std::env::var("PATH").unwrap_or_default();
    let shell = std::env::var("SHELL").unwrap_or_default();
    install_shell_path_for(&exe, &path_env, &shell);
}

/// Make the graphmind at `exe` reachable on PATH for `shell` (a $SHELL value):
/// 1. remove the `~/.graphmind/bin` line older versions added when no binary lives there;
/// 2. if PATH already reaches it (`already_on_path`), change nothing — otherwise every upgrade
///    would append another line, silently through auto-setup;
/// 3. otherwise write one marked line to the profile for the shell: `.zshenv` for zsh (every
///    zsh reads it, interactive or not), `.bashrc` for bash, `.profile` for anything else.
#[doc(hidden)]
pub fn install_shell_path_for(exe: &Path, path_env: &str, shell: &str) {
    remove_dead_legacy_line();

    if let Some(found) = already_on_path(exe, path_env) {
        println!("    {} PATH already includes graphmind ({})", "✓".green(), found.display());
        return;
    }

    let bin_dir = bin_dir_for(exe);
    let bin_dir_str = bin_dir.to_string_lossy();
    let export_line = format!("export PATH=\"{}:$PATH\"  {}", bin_dir_str, PATH_MARKER);
    let profile = match shell.rsplit('/').next().unwrap_or("") {
        "zsh" => ".zshenv",
        "bash" => ".bashrc",
        _ => ".profile",
    };
    let path = home_dir().join(profile);
    let content = fs::read_to_string(&path).unwrap_or_default();
    if content.contains(&format!("\"{}:$PATH\"", bin_dir_str)) {
        println!("    {} PATH already configured in ~/{}", "✓".green(), profile);
        return;
    }
    let new_content = if content.is_empty() {
        format!("{}\n", export_line)
    } else {
        format!("{}\n{}\n", content.trim_end(), export_line)
    };
    if let Err(e) = fs::write(&path, new_content) {
        println!("    {} failed to update {}: {e}", "✗".red(), profile);
    } else {
        println!("    {} added {} to ~/{}", "✓".green(), bin_dir.display(), profile);
        println!("    {} restart your shell or run: source ~/{}", "→".cyan(), profile);
    }
}

/// Drop the legacy `~/.graphmind/bin` PATH line when that folder has no graphmind binary.
fn remove_dead_legacy_line() {
    if home_dir().join(".graphmind").join("bin").join("graphmind").exists() {
        return;
    }
    let legacy = legacy_export_line();
    let removed = remove_profile_lines(&|line| line == legacy);
    if !removed.is_empty() {
        println!(
            "    {} removed unused ~/.graphmind/bin PATH line from: {}",
            "✓".green(),
            removed.join(", ")
        );
    }
}

fn install_shell_path_windows() {
    let install_dir = home_dir().join(".graphmind").join("bin");
    let install_dir_str = install_dir.to_string_lossy().to_string();

    // 1. Add to user PATH via registry (persistent, non-interactive shells)
    let ps_set_path = format!(
        r#"$current = [Environment]::GetEnvironmentVariable('PATH', 'User'); if ($current -notlike '*{0}*') {{ [Environment]::SetEnvironmentVariable('PATH', '{0};' + $current, 'User'); Write-Output 'updated' }} else {{ Write-Output 'already' }}"#,
        install_dir_str
    );
    match std::process::Command::new("powershell")
        .args(["-NoProfile", "-Command", &ps_set_path])
        .output()
    {
        Ok(out) if out.status.success() => {
            let result = String::from_utf8_lossy(&out.stdout).trim().to_string();
            if result == "already" {
                println!("    {} PATH already configured (user environment)", "✓".green());
            } else {
                println!("    {} added to user PATH (registry)", "✓".green());
            }
        }
        Ok(out) => {
            let err = String::from_utf8_lossy(&out.stderr);
            println!("    {} failed to set user PATH: {}", "✗".red(), err.trim());
        }
        Err(e) => {
            println!("    {} powershell not available: {e}", "✗".red());
        }
    }

    // 2. Also add to PowerShell profile for interactive sessions
    let ps_get_profile = "$PROFILE";
    if let Ok(out) = std::process::Command::new("powershell")
        .args(["-NoProfile", "-Command", ps_get_profile])
        .output()
    {
        if out.status.success() {
            let profile_path = String::from_utf8_lossy(&out.stdout).trim().to_string();
            let profile_path = std::path::PathBuf::from(&profile_path);
            let add_line = format!("$env:PATH = '{};' + $env:PATH", install_dir_str);
            let content = if profile_path.exists() {
                fs::read_to_string(&profile_path).unwrap_or_default()
            } else {
                String::new()
            };
            if !content.contains(&install_dir_str) {
                if let Some(parent) = profile_path.parent() {
                    fs::create_dir_all(parent).ok();
                }
                let new_content = if content.is_empty() {
                    format!("{}\n", add_line)
                } else {
                    format!("{}\n{}\n", content.trim_end(), add_line)
                };
                if let Err(e) = fs::write(&profile_path, new_content) {
                    println!("    {} failed to update PowerShell profile: {e}", "✗".red());
                } else {
                    println!("    {} added to PowerShell profile", "✓".green());
                    println!("    {} restart PowerShell to pick up changes", "→".cyan());
                }
            }
        }
    }
}

#[doc(hidden)]
pub fn uninstall_shell_path() {
    if cfg!(target_os = "windows") {
        uninstall_shell_path_windows();
    } else {
        uninstall_shell_path_unix();
    }
}

fn uninstall_shell_path_unix() {
    let legacy = legacy_export_line();
    let updated = remove_profile_lines(&|line| line == legacy || line.trim_end().ends_with(PATH_MARKER));
    if updated.is_empty() {
        println!("    {} no graphmind PATH line found", "✓".green());
    } else {
        println!("    {} removed graphmind PATH line from: {}", "✓".green(), updated.join(", "));
    }
}

/// Remove matching lines from the shell profiles graphmind has ever written to (backup first),
/// collapsing a blank line left orphaned. Returns the profiles changed.
fn remove_profile_lines(matches: &dyn Fn(&str) -> bool) -> Vec<&'static str> {
    let profiles: &[&'static str] = &[".zshenv", ".zshrc", ".bashrc", ".profile"];
    let mut updated = Vec::new();
    for profile in profiles {
        let path = home_dir().join(profile);
        if !path.exists() {
            continue;
        }
        let content = fs::read_to_string(&path).unwrap_or_default();
        if !content.lines().any(matches) {
            continue;
        }

        // Backup before modifying
        let ts = chrono::Utc::now().format("%Y%m%d_%H%M%S");
        let backup = path.with_extension(format!("{}.{ts}.bak", profile.trim_start_matches('.')));
        fs::copy(&path, &backup).ok();

        let lines: Vec<&str> = content.lines().collect();
        let mut new_lines: Vec<&str> = Vec::with_capacity(lines.len());
        let mut prev_blank = false;
        for line in lines {
            if matches(line) {
                // Drop the graphmind line; collapse an orphaned trailing blank line.
                continue;
            }
            if line.trim().is_empty() {
                if prev_blank {
                    continue;
                }
                prev_blank = true;
            } else {
                prev_blank = false;
            }
            new_lines.push(line);
        }
        let mut new_content = new_lines.join("\n");
        if !new_content.is_empty() {
            new_content.push('\n');
        }

        if let Err(e) = fs::write(&path, new_content) {
            println!("    {} failed to update {}: {e}", "✗".red(), profile);
        } else {
            updated.push(*profile);
        }
    }

    updated
}

fn uninstall_shell_path_windows() {
    let install_dir = home_dir().join(".graphmind").join("bin");
    let install_dir_str = install_dir.to_string_lossy().to_string();

    // 1. Remove from user PATH via registry
    let ps_remove_path = format!(
        r#"$current = [Environment]::GetEnvironmentVariable('PATH', 'User'); if ($current -like '*{0}*') {{ $parts = $current.Split(';') | Where-Object {{ $_ -ne '{0}' }}; [Environment]::SetEnvironmentVariable('PATH', ($parts -join ';'), 'User'); Write-Output 'removed' }} else {{ Write-Output 'absent' }}"#,
        install_dir_str
    );
    match std::process::Command::new("powershell")
        .args(["-NoProfile", "-Command", &ps_remove_path])
        .output()
    {
        Ok(out) if out.status.success() => {
            let result = String::from_utf8_lossy(&out.stdout).trim().to_string();
            if result == "absent" {
                println!("    {} PATH already clean (user environment)", "✓".green());
            } else {
                println!("    {} removed from user PATH (registry)", "✓".green());
            }
        }
        Ok(out) => {
            let err = String::from_utf8_lossy(&out.stderr);
            println!("    {} failed to update user PATH: {}", "✗".red(), err.trim());
        }
        Err(e) => {
            println!("    {} powershell not available: {e}", "✗".red());
        }
    }

    // 2. Remove the matching line from $PROFILE
    let ps_get_profile = "$PROFILE";
    if let Ok(out) = std::process::Command::new("powershell")
        .args(["-NoProfile", "-Command", ps_get_profile])
        .output()
    {
        if out.status.success() {
            let profile_path = String::from_utf8_lossy(&out.stdout).trim().to_string();
            let profile_path = std::path::PathBuf::from(&profile_path);
            let target_line = format!("$env:PATH = '{};' + $env:PATH", install_dir_str);
            if profile_path.exists() {
                let content = fs::read_to_string(&profile_path).unwrap_or_default();
                if content.contains(&target_line) {
                    let ts = chrono::Utc::now().format("%Y%m%d_%H%M%S");
                    let backup = profile_path.with_extension(format!("ps1.{ts}.bak"));
                    fs::copy(&profile_path, &backup).ok();

                    let new_content: String = content
                        .lines()
                        .filter(|l| *l != target_line)
                        .collect::<Vec<_>>()
                        .join("\n");
                    if let Err(e) = fs::write(&profile_path, format!("{new_content}\n")) {
                        println!("    {} failed to update PowerShell profile: {e}", "✗".red());
                    } else {
                        println!("    {} removed from PowerShell profile", "✓".green());
                    }
                }
            }
        }
    }
}

//! Shell PATH configuration for `graphmind setup` and `uninstall`: make the installed binary
//! reachable, once, only when it isn't already, and remove exactly the lines setup added.

use super::home_dir;
use colored::Colorize;
use graphmind_config::back_up;
use std::fs;
use std::path::{Path, PathBuf};

/// What the shell PATH step did, for setup's summary.
#[doc(hidden)]
#[derive(Debug, PartialEq, Eq)]
pub enum ShellPath {
    /// PATH already reaches graphmind: nothing written.
    AlreadyOnPath,
    /// The profile the value names (`~/.zshenv`) already has the line, but this shell's PATH
    /// doesn't reach graphmind yet (it started before the line was added): nothing written.
    InProfile(String),
    /// The line was added where the value says (`~/.zshenv`, `user PATH`).
    Configured(String),
    /// A profile was left unchanged (or PATH couldn't be set): this folder has to go on PATH by hand.
    LeftUnchanged(PathBuf),
}

#[doc(hidden)]
pub fn install_shell_path() -> ShellPath {
    if cfg!(target_os = "windows") {
        install_shell_path_windows()
    } else {
        install_shell_path_unix()
    }
}

/// Marks PATH lines written by setup, so uninstall removes exactly what setup added.
const PATH_MARKER: &str = "# added by graphmind setup";

/// The PATH line older CLI versions (<= 0.2.215) and desktop app versions wrote for
/// `~/.graphmind/bin`. Matched in every profile `remove_profile_lines` scans.
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

fn install_shell_path_unix() -> ShellPath {
    let exe = match std::env::current_exe() {
        Ok(exe) => exe,
        Err(e) => {
            println!("    {} can't locate the graphmind binary ({e}); PATH left unchanged", "⊘".yellow());
            return ShellPath::LeftUnchanged(home_dir().join(".graphmind").join("bin"));
        }
    };
    let path_env = std::env::var("PATH").unwrap_or_default();
    let shell = std::env::var("SHELL").unwrap_or_default();
    install_shell_path_for(&exe, &path_env, &shell)
}

/// Make the graphmind at `exe` reachable on PATH for `shell` (a $SHELL value):
/// 1. remove the `~/.graphmind/bin` line older versions added when no binary lives there;
/// 2. if PATH already reaches it (`already_on_path`), change nothing — otherwise every upgrade
///    would append another line, silently through auto-setup;
/// 3. otherwise write one marked line to the profile for the shell: `.zshenv` for zsh (every
///    zsh reads it, interactive or not), `.bashrc` for bash, `.profile` for anything else. A
///    missing profile is created; an existing one is backed up first, and one that can't be read
///    as text or backed up is left unchanged (see [`append_line`]).
#[doc(hidden)]
pub fn install_shell_path_for(exe: &Path, path_env: &str, shell: &str) -> ShellPath {
    remove_dead_legacy_line();

    if let Some(found) = already_on_path(exe, path_env) {
        println!("    {} PATH already includes graphmind ({})", "✓".green(), found.display());
        return ShellPath::AlreadyOnPath;
    }

    let bin_dir = bin_dir_for(exe);
    let bin_dir_str = bin_dir.to_string_lossy();
    let export_line = format!("export PATH=\"{}:$PATH\"  {}", bin_dir_str, PATH_MARKER);
    let profile = match shell.rsplit('/').next().unwrap_or("") {
        "zsh" => ".zshenv",
        "bash" => ".bashrc",
        _ => ".profile",
    };
    let needle = format!("\"{}:$PATH\"", bin_dir_str);
    match append_line(&home_dir().join(profile), &export_line, |content| content.contains(&needle)) {
        Ok(Appended::AlreadyPresent) => {
            println!("    {} PATH already configured in ~/{} (open a new shell)", "✓".green(), profile);
            ShellPath::InProfile(format!("~/{profile}"))
        }
        Ok(Appended::Written) => {
            println!("    {} added {} to ~/{}", "✓".green(), bin_dir.display(), profile);
            println!("    {} restart your shell or run: source ~/{}", "→".cyan(), profile);
            ShellPath::Configured(format!("~/{profile}"))
        }
        Err(reason) => {
            println!("    {} {reason}", "⊘".yellow());
            println!("    {} add {} to PATH yourself", "→".cyan(), bin_dir.display());
            ShellPath::LeftUnchanged(bin_dir)
        }
    }
}

/// What [`append_line`] did with a profile.
enum Appended {
    /// The profile already has the line: nothing written, nothing backed up.
    AlreadyPresent,
    /// The line was added at the end, after backing up an existing profile.
    Written,
}

/// Add `line` to the end of the shell profile at `path` unless `already_present` says the content
/// has it. An existing profile is backed up first; a missing one is created. `Err` holds the
/// reason when the profile is left unchanged: it can't be read as text (permissions, not UTF-8),
/// can't be backed up, or can't be rewritten. Rewriting a profile that couldn't be read would
/// replace it with just the line, so it is never attempted.
fn append_line(path: &Path, line: &str, already_present: impl Fn(&str) -> bool) -> Result<Appended, String> {
    let content = match read_profile(path)? {
        Some(content) if already_present(&content) => return Ok(Appended::AlreadyPresent),
        Some(content) => {
            back_up(path)?;
            content
        }
        None => {
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent)
                    .map_err(|e| format!("can't create {} ({e}); left unchanged", parent.display()))?;
            }
            String::new()
        }
    };
    let new_content = if content.is_empty() {
        format!("{line}\n")
    } else {
        format!("{}\n{line}\n", content.trim_end())
    };
    graphmind_config::write_atomic(path, new_content.as_bytes())
        .map_err(|e| format!("failed to update {} ({e}); left unchanged", path.display()))?;
    Ok(Appended::Written)
}

/// A shell profile's content: `Ok(None)` when it doesn't exist, `Err` with the reason when it
/// exists but can't be read as text (permissions, not UTF-8).
fn read_profile(path: &Path) -> Result<Option<String>, String> {
    match fs::read_to_string(path) {
        Ok(content) => Ok(Some(content)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(format!("can't read {} ({e}); left unchanged", path.display())),
    }
}

/// Drop the legacy `~/.graphmind/bin` PATH line when that folder has no graphmind binary.
/// Profiles that can't be read or backed up keep it: a dead PATH entry is harmless, and the
/// install step reports the profile it writes to.
fn remove_dead_legacy_line() {
    if home_dir().join(".graphmind").join("bin").join("graphmind").exists() {
        return;
    }
    let legacy = legacy_export_line();
    let (removed, _) = remove_profile_lines(&|line| line == legacy);
    if !removed.is_empty() {
        println!(
            "    {} removed unused ~/.graphmind/bin PATH line from: {}",
            "✓".green(),
            removed.join(", ")
        );
    }
}

fn install_shell_path_windows() -> ShellPath {
    let install_dir = home_dir().join(".graphmind").join("bin");
    let install_dir_str = install_dir.to_string_lossy().to_string();

    // 1. Add to user PATH via registry (persistent, non-interactive shells)
    let ps_set_path = format!(
        r#"$current = [Environment]::GetEnvironmentVariable('PATH', 'User'); if ($current -notlike '*{0}*') {{ [Environment]::SetEnvironmentVariable('PATH', '{0};' + $current, 'User'); Write-Output 'updated' }} else {{ Write-Output 'already' }}"#,
        install_dir_str
    );
    let mut outcome = match std::process::Command::new("powershell")
        .args(["-NoProfile", "-Command", &ps_set_path])
        .output()
    {
        Ok(out) if out.status.success() => {
            let result = String::from_utf8_lossy(&out.stdout).trim().to_string();
            if result == "already" {
                println!("    {} PATH already configured (user environment)", "✓".green());
                ShellPath::AlreadyOnPath
            } else {
                println!("    {} added to user PATH (registry)", "✓".green());
                ShellPath::Configured("user PATH".to_string())
            }
        }
        Ok(out) => {
            let err = String::from_utf8_lossy(&out.stderr);
            println!("    {} failed to set user PATH: {}", "✗".red(), err.trim());
            ShellPath::LeftUnchanged(install_dir.clone())
        }
        Err(e) => {
            println!("    {} powershell not available: {e}", "✗".red());
            ShellPath::LeftUnchanged(install_dir.clone())
        }
    };

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
            match append_line(&profile_path, &add_line, |content| content.contains(&install_dir_str)) {
                Ok(Appended::AlreadyPresent) => {}
                Ok(Appended::Written) => {
                    println!("    {} added to PowerShell profile", "✓".green());
                    println!("    {} restart PowerShell to pick up changes", "→".cyan());
                    if outcome == ShellPath::AlreadyOnPath {
                        outcome = ShellPath::Configured(profile_path.display().to_string());
                    }
                }
                // The user PATH set above already reaches graphmind in new shells; only when
                // that failed too does the folder have to go on PATH by hand.
                Err(reason) => {
                    println!("    {} {reason}", "⊘".yellow());
                    if matches!(outcome, ShellPath::LeftUnchanged(_)) {
                        println!("    {} add {} to PATH yourself", "→".cyan(), install_dir.display());
                    }
                }
            }
        }
    }
    outcome
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
    let (updated, left_alone) =
        remove_profile_lines(&|line| line == legacy || line.trim_end().ends_with(PATH_MARKER));
    for reason in &left_alone {
        println!("    {} {reason}", "⊘".yellow());
    }
    if !updated.is_empty() {
        println!("    {} removed graphmind PATH line from: {}", "✓".green(), updated.join(", "));
    } else if left_alone.is_empty() {
        println!("    {} no graphmind PATH line found", "✓".green());
    }
}

/// Remove matching lines from the shell profiles graphmind has ever written to (backup first),
/// collapsing a blank line left orphaned. Returns the profiles changed, and the reasons for those
/// left unchanged (can't be read or backed up).
fn remove_profile_lines(matches: &dyn Fn(&str) -> bool) -> (Vec<&'static str>, Vec<String>) {
    let profiles: &[&'static str] = &[".zshenv", ".zshrc", ".bashrc", ".bash_profile", ".profile"];
    let mut updated = Vec::new();
    let mut left_alone = Vec::new();
    for profile in profiles {
        let path = home_dir().join(profile);
        let content = match read_profile(&path) {
            Ok(Some(content)) => content,
            Ok(None) => continue,
            Err(reason) => {
                left_alone.push(reason);
                continue;
            }
        };
        if !content.lines().any(matches) {
            continue;
        }
        if let Err(reason) = back_up(&path) {
            left_alone.push(reason);
            continue;
        }

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

        if let Err(e) = graphmind_config::write_atomic(&path, new_content.as_bytes()) {
            println!("    {} failed to update {}: {e}", "✗".red(), profile);
        } else {
            updated.push(*profile);
        }
    }

    (updated, left_alone)
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
            match read_profile(&profile_path) {
                Err(reason) => println!("    {} {reason}", "⊘".yellow()),
                Ok(Some(content)) if content.contains(&target_line) => {
                    if let Err(reason) = back_up(&profile_path) {
                        println!("    {} {reason}", "⊘".yellow());
                        return;
                    }

                    let new_content: String = content
                        .lines()
                        .filter(|l| *l != target_line)
                        .collect::<Vec<_>>()
                        .join("\n");
                    if let Err(e) = graphmind_config::write_atomic(&profile_path, format!("{new_content}\n").as_bytes()) {
                        println!("    {} failed to update PowerShell profile: {e}", "✗".red());
                    } else {
                        println!("    {} removed from PowerShell profile", "✓".green());
                    }
                }
                Ok(_) => {}
            }
        }
    }
}

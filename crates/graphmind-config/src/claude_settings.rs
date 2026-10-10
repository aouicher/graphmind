//! Repairs to ~/.claude/settings.json for what older graphmind versions wrote there, shared by
//! `graphmind setup` and the desktop app. Nothing here prints: callers report the result.

use serde_json::{json, Value};

use crate::mcp_clients::{read_json_config, write_config, ConfigRead};
use crate::paths::home_dir;

/// What [`repair_claude_settings`] changed.
#[derive(Debug, PartialEq)]
pub struct SettingsRepair {
    pub path: PathRepair,
    /// The old `mcpServers.graphmind` entry was removed.
    pub legacy_entry_removed: bool,
}

/// Outcome of removing the `~/.graphmind/bin` segment that graphmind <= v0.2.211 injected into
/// the global `env.PATH` of `~/.claude/settings.json` (issue #105).
///
/// That key applies to every Bash tool call in Claude Code, and the old code seeded it from a
/// hardcoded `/usr/local/bin:/usr/bin:/bin` when absent — so users on nvm/asdf/mise/Homebrew-ARM
/// silently lost those directories.
///
/// The repair is deliberately careful, because a user may have edited the value by hand since:
///   - only the exact `~/.graphmind/bin` segment is dropped, wherever it sits;
///   - if every remaining segment is a standard system directory, `env.PATH` is removed entirely
///     so Claude Code falls back to the inherited shell PATH. Such a value carries no information
///     the shell would not already provide, and keeping it truncated is actively harmful: it
///     still shadows the user's real PATH (pyenv/bun/cargo/plugin bins), which is the whole bug;
///   - any segment outside that set may be a deliberate user choice, so the key is KEPT with our
///     segment removed. We cannot tell a purposeful entry apart from one the user added to work
///     around this very bug, and overwriting intent is the mistake #105 is about. The caller
///     warns instead;
///   - an `env` object left empty is removed too, to avoid dead keys.
#[derive(Debug, PartialEq)]
pub enum PathRepair {
    /// Our segment was not present — config untouched.
    NotNeeded,
    /// Only system dirs remained, so `env.PATH` was removed entirely and the inherited shell PATH
    /// now wins. Fully fixed.
    KeyRemoved,
    /// User-specific segments remained, so the key survives minus our segment. Still overrides
    /// the shell PATH — needs a heads-up.
    Truncated { remaining: String },
}

/// Repair ~/.claude/settings.json: remove the `env.PATH` pollution of graphmind <= v0.2.211 and,
/// when `remove_legacy_entry` is set, the `mcpServers.graphmind` entry older versions wrote there
/// (Claude Code reads user-scope servers from ~/.claude.json). Callers set it only once the
/// user-scope entry is in place, or on uninstall, so Claude Code is never left without one.
///
/// A missing file needs nothing. A file that can't be read or parsed, or can't be written, is
/// left alone and the reason returned.
pub fn repair_claude_settings(remove_legacy_entry: bool) -> Result<SettingsRepair, String> {
    let settings_path = home_dir().join(".claude").join("settings.json");
    let unchanged = SettingsRepair { path: PathRepair::NotNeeded, legacy_entry_removed: false };
    let mut config = match read_json_config(&settings_path, false) {
        ConfigRead::Missing => return Ok(unchanged),
        ConfigRead::Parsed { value, .. } => value,
        ConfigRead::Unparseable(reason) => return Err(reason),
    };
    let graphmind_bin_dir = home_dir().join(".graphmind").join("bin").to_string_lossy().to_string();

    // We deliberately do NOT set a global env.PATH: it applies to every Bash tool call in Claude
    // Code. The hook scripts already self-prefix PATH (see claude_hook.rs), so hooks resolve
    // `graphmind` without any settings.json help. See issue #105.
    let repair = SettingsRepair {
        path: repair_claude_settings_path(&mut config, &graphmind_bin_dir),
        legacy_entry_removed: remove_legacy_entry && remove_settings_mcp_entry(&mut config),
    };
    if repair == unchanged {
        return Ok(repair);
    }
    write_config(&settings_path, &config)?;
    Ok(repair)
}

/// Remove `mcpServers.graphmind`, and `mcpServers` itself when that leaves it empty.
fn remove_settings_mcp_entry(config: &mut Value) -> bool {
    let Some(root) = config.as_object_mut() else {
        return false;
    };
    let Some(servers) = root.get_mut("mcpServers").and_then(Value::as_object_mut) else {
        return false;
    };
    if servers.remove("graphmind").is_none() {
        return false;
    }
    if servers.is_empty() {
        root.remove("mcpServers");
    }
    true
}

fn repair_claude_settings_path(config: &mut Value, graphmind_bin_dir: &str) -> PathRepair {
    let Some(env) = config.get_mut("env").and_then(|e| e.as_object_mut()) else {
        return PathRepair::NotNeeded;
    };
    let Some(current) = env.get("PATH").and_then(|p| p.as_str()) else {
        return PathRepair::NotNeeded;
    };

    let kept: Vec<&str> = current
        .split(':')
        .filter(|seg| !seg.is_empty() && *seg != graphmind_bin_dir)
        .collect();

    if kept.len() == current.split(':').filter(|s| !s.is_empty()).count() {
        return PathRepair::NotNeeded; // our segment was not there
    }

    // Directories a login shell provides anyway. A PATH made only of these adds
    // nothing but still shadows the real environment, so the key is dropped.
    // Includes both Homebrew prefixes (ARM and x86).
    const SYSTEM_DIRS: [&str; 8] = [
        "/usr/local/bin",
        "/usr/local/sbin",
        "/usr/bin",
        "/bin",
        "/usr/sbin",
        "/sbin",
        "/opt/homebrew/bin",
        "/opt/homebrew/sbin",
    ];

    let outcome = if kept.iter().all(|seg| SYSTEM_DIRS.contains(seg)) {
        env.remove("PATH");
        PathRepair::KeyRemoved
    } else {
        let joined = kept.join(":");
        env.insert("PATH".to_string(), json!(joined.clone()));
        PathRepair::Truncated { remaining: joined }
    };

    if env.is_empty() {
        config.as_object_mut().unwrap().remove("env");
    }
    outcome
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn repair_strips_our_segment_and_keeps_user_edits() {
        let bin_dir = "/home/user/.graphmind/bin";
        let nvm = "/home/user/.nvm/versions/node/v20.11.0/bin";
        let mut json = json!({ "env": { "PATH": format!("{bin_dir}:{nvm}:/usr/local/bin:/usr/bin:/bin") } });

        // nvm is user-specific, so the key survives — and the caller is told.
        assert_eq!(
            repair_claude_settings_path(&mut json, bin_dir),
            PathRepair::Truncated { remaining: format!("{nvm}:/usr/local/bin:/usr/bin:/bin") }
        );
        let path = json["env"]["PATH"].as_str().unwrap();
        assert!(!path.split(':').any(|s| s == bin_dir), "got: {path}");
        assert!(path.starts_with(nvm), "user edit must be preserved: {path}");
    }

    #[test]
    fn repair_removes_key_when_only_legacy_default_remains() {
        let bin_dir = "/home/user/.graphmind/bin";
        let mut json = json!({ "env": { "PATH": format!("{bin_dir}:/usr/local/bin:/usr/bin:/bin") } });
        assert_eq!(repair_claude_settings_path(&mut json, bin_dir), PathRepair::KeyRemoved);
        // The whole `env` object goes away, so the inherited shell PATH wins.
        assert!(json.get("env").is_none(), "got: {json}");
    }

    /// A real-world pre-fix install where the user had hand-added Homebrew-ARM and the sbin dirs.
    /// Those are all system dirs, so the key must still go.
    #[test]
    fn repair_removes_key_when_only_system_dirs_remain() {
        let bin_dir = "/Users/someone/.graphmind/bin";
        let mut json = json!({
            "env": {
                "PATH": format!("{bin_dir}:/opt/homebrew/bin:/usr/local/bin:/usr/bin:/bin:/usr/sbin:/sbin"),
                "SOME_OTHER_VAR": "keep-me"
            }
        });
        assert_eq!(repair_claude_settings_path(&mut json, bin_dir), PathRepair::KeyRemoved);
        assert!(json["env"]["PATH"].is_null(), "PATH should be gone: {json}");
        assert_eq!(json["env"]["SOME_OTHER_VAR"], "keep-me");
    }

    #[test]
    fn repair_is_noop_when_segment_absent() {
        let mut json = json!({ "env": { "PATH": "/opt/homebrew/bin:/usr/bin" } });
        assert_eq!(repair_claude_settings_path(&mut json, "/home/user/.graphmind/bin"), PathRepair::NotNeeded);
        assert_eq!(json["env"]["PATH"], "/opt/homebrew/bin:/usr/bin");
    }

    #[test]
    fn repair_strips_segment_in_middle() {
        let bin_dir = "/home/user/.graphmind/bin";
        let mut json = json!({ "env": { "PATH": format!("/opt/homebrew/bin:{bin_dir}:/usr/bin") } });
        assert_eq!(repair_claude_settings_path(&mut json, bin_dir), PathRepair::KeyRemoved);
        assert!(json.get("env").is_none(), "got: {json}");
    }

    #[test]
    fn legacy_entry_removal_keeps_other_servers() {
        let mut json = json!({ "mcpServers": { "graphmind": {}, "other": {} } });
        assert!(remove_settings_mcp_entry(&mut json));
        assert_eq!(json, json!({ "mcpServers": { "other": {} } }));
        let mut json = json!({ "mcpServers": { "graphmind": {} }, "theme": "dark" });
        assert!(remove_settings_mcp_entry(&mut json));
        assert_eq!(json, json!({ "theme": "dark" }));
        assert!(!remove_settings_mcp_entry(&mut json));
    }
}

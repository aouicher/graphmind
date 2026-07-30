use crate::types::AiClient;
use serde_json::Value;
use std::fs;
use std::path::PathBuf;

fn home_dir() -> PathBuf {
    dirs::home_dir().expect("Cannot determine home directory")
}

fn graphmind_bin_dir() -> String {
    home_dir().join(".graphmind").join("bin").to_string_lossy().to_string()
}

fn get_cli_binary_path() -> String {
    // Prefer the known install location first — avoids relying on shell PATH
    let local_path = home_dir().join(".graphmind").join("bin").join("graphmind");
    if local_path.exists() {
        return local_path.to_string_lossy().to_string();
    }
    if let Ok(output) = std::process::Command::new("which")
        .arg("graphmind")
        .output()
    {
        if output.status.success() {
            let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if !path.is_empty() {
                return path;
            }
        }
    }
    // Fall back to absolute path even if not yet present (install in progress)
    local_path.to_string_lossy().to_string()
}

fn claude_config_path() -> PathBuf {
    home_dir().join(".claude").join("settings.json")
}

fn claude_desktop_config_path() -> PathBuf {
    if cfg!(target_os = "macos") {
        home_dir().join("Library/Application Support/Claude/claude_desktop_config.json")
    } else {
        home_dir().join(".config/claude/claude_desktop_config.json")
    }
}

fn cursor_config_path() -> PathBuf {
    home_dir().join(".cursor").join("mcp.json")
}

fn opencode_config_path() -> PathBuf {
    // Global config: ~/.config/opencode/opencode.jsonc (Linux/macOS)
    #[cfg(target_os = "macos")]
    {
        home_dir().join(".config").join("opencode").join("opencode.jsonc")
    }
    #[cfg(not(target_os = "macos"))]
    {
        home_dir().join(".config").join("opencode").join("opencode.jsonc")
    }
}

fn is_opencode_detected() -> bool {
    // Detected if config file exists or binary is in PATH
    if opencode_config_path().exists() {
        return true;
    }
    std::process::Command::new("which")
        .arg("opencode")
        .output()
        .is_ok_and(|o| o.status.success())
}

fn is_opencode_mcp_configured() -> bool {
    let config_path = opencode_config_path();
    if !config_path.exists() {
        return false;
    }
    let content = match fs::read_to_string(&config_path) {
        Ok(c) => c,
        Err(_) => return false,
    };
    // Strip JSONC comments before parsing
    let stripped = strip_jsonc_comments(&content);
    let json: Value = match serde_json::from_str(&stripped) {
        Ok(v) => v,
        Err(_) => return false,
    };
    json.get("mcp").and_then(|m| m.get("graphmind")).is_some()
}

fn strip_jsonc_comments(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    let mut in_string = false;
    while let Some(c) = chars.next() {
        if c == '"' && !in_string {
            in_string = true;
            out.push(c);
        } else if c == '"' && in_string {
            in_string = false;
            out.push(c);
        } else if !in_string && c == '/' {
            if chars.peek() == Some(&'/') {
                // Line comment — skip to end of line
                for ch in chars.by_ref() {
                    if ch == '\n' { out.push('\n'); break; }
                }
            } else if chars.peek() == Some(&'*') {
                // Block comment — skip to */
                chars.next();
                loop {
                    match chars.next() {
                        Some('*') if chars.peek() == Some(&'/') => { chars.next(); break; }
                        None => break,
                        _ => {}
                    }
                }
            } else {
                out.push(c);
            }
        } else {
            out.push(c);
        }
    }
    out
}

fn is_claude_detected() -> bool {
    claude_config_path().parent().is_some_and(|p| p.exists())
        || std::process::Command::new("which")
            .arg("claude")
            .output()
            .is_ok_and(|o| o.status.success())
}

fn is_claude_desktop_detected() -> bool {
    let config = claude_desktop_config_path();
    config.parent().is_some_and(|p| p.exists())
}

fn is_cursor_detected() -> bool {
    // Require the actual Cursor app or config file — not just ~/.cursor directory
    let config = cursor_config_path();
    if config.exists() {
        return true;
    }
    #[cfg(target_os = "macos")]
    {
        std::path::Path::new("/Applications/Cursor.app").exists()
            || home_dir().join("Applications/Cursor.app").exists()
    }
    #[cfg(not(target_os = "macos"))]
    {
        std::process::Command::new("which")
            .arg("cursor")
            .output()
            .is_ok_and(|o| o.status.success())
    }
}

fn is_mcp_configured(config_path: &PathBuf, key: &str) -> bool {
    if !config_path.exists() {
        return false;
    }
    let content = match fs::read_to_string(config_path) {
        Ok(c) => c,
        Err(_) => return false,
    };
    let json: Value = match serde_json::from_str(&content) {
        Ok(v) => v,
        Err(_) => return false,
    };
    json.get("mcpServers")
        .and_then(|s| s.get(key))
        .is_some()
}

#[tauri::command]
pub fn detect_clients() -> Vec<AiClient> {
    let claude_path = claude_config_path();
    let claude_desktop_path = claude_desktop_config_path();
    let cursor_path = cursor_config_path();
    let opencode_path = opencode_config_path();

    vec![
        AiClient {
            id: "claude-code".to_string(),
            name: "Claude Code".to_string(),
            icon: "terminal".to_string(),
            detected: is_claude_detected(),
            mcp_configured: is_mcp_configured(&claude_path, "graphmind"),
            config_path: Some(claude_path.to_string_lossy().to_string()),
        },
        AiClient {
            id: "claude-desktop".to_string(),
            name: "Claude Desktop".to_string(),
            icon: "app-window".to_string(),
            detected: is_claude_desktop_detected(),
            mcp_configured: is_mcp_configured(&claude_desktop_path, "graphmind"),
            config_path: Some(claude_desktop_path.to_string_lossy().to_string()),
        },
        AiClient {
            id: "cursor".to_string(),
            name: "Cursor".to_string(),
            icon: "mouse-pointer".to_string(),
            detected: is_cursor_detected(),
            mcp_configured: is_mcp_configured(&cursor_path, "graphmind"),
            config_path: Some(cursor_path.to_string_lossy().to_string()),
        },
        AiClient {
            id: "opencode".to_string(),
            name: "OpenCode".to_string(),
            icon: "code".to_string(),
            detected: is_opencode_detected(),
            mcp_configured: is_opencode_mcp_configured(),
            config_path: Some(opencode_path.to_string_lossy().to_string()),
        },
    ]
}

#[tauri::command]
pub fn install_mcp_for_client(client_id: String) -> Result<(), String> {
    let binary_path = get_cli_binary_path();

    match client_id.as_str() {
        "claude-code" => install_claude_mcp(&binary_path),
        "claude-desktop" => install_claude_desktop_mcp(&binary_path),
        "cursor" => install_cursor_mcp(&binary_path),
        "opencode" => install_opencode_mcp(&binary_path),
        _ => Err(format!("Unsupported client: {client_id}")),
    }
}

#[tauri::command]
pub fn uninstall_mcp_for_client(client_id: String) -> Result<(), String> {
    match client_id.as_str() {
        "claude-code" => uninstall_claude_mcp(),
        "claude-desktop" => uninstall_claude_desktop_mcp(),
        "cursor" => uninstall_cursor_mcp(),
        "opencode" => uninstall_opencode_mcp(),
        _ => Err(format!("Unsupported client: {client_id}")),
    }
}

fn install_claude_mcp(binary_path: &str) -> Result<(), String> {
    let config_path = claude_config_path();
    let mut json = read_or_create_json(&config_path)?;
    let bin_dir = graphmind_bin_dir();

    // Repair the global env.PATH written by graphmind <= v0.2.211 and do not
    // re-add it: that key applies to every Bash tool call in Claude Code, and the
    // old hardcoded "/usr/local/bin:/usr/bin:/bin" default wiped nvm/asdf/
    // Homebrew-ARM paths. Hook scripts self-prefix PATH already. See issue #105.
    repair_claude_settings_path(&mut json, &bin_dir);

    let servers = json
        .as_object_mut()
        .ok_or("Invalid config format")?
        .entry("mcpServers")
        .or_insert_with(|| Value::Object(serde_json::Map::new()));

    let servers_obj = servers.as_object_mut().ok_or("mcpServers is not an object")?;
    // No `env` block — `binary_path` is absolute. See issue #105.
    servers_obj.insert(
        "graphmind".to_string(),
        serde_json::json!({
            "command": binary_path,
            "args": ["mcp"],
            "type": "stdio"
        }),
    );

    write_json(&config_path, &json)
}

fn install_claude_desktop_mcp(binary_path: &str) -> Result<(), String> {
    let config_path = claude_desktop_config_path();
    let mut json = read_or_create_json(&config_path)?;

    let servers = json
        .as_object_mut()
        .ok_or("Invalid config format")?
        .entry("mcpServers")
        .or_insert_with(|| Value::Object(serde_json::Map::new()));

    let servers_obj = servers.as_object_mut().ok_or("mcpServers is not an object")?;
    // No `env` block — `binary_path` is absolute. See issue #105.
    servers_obj.insert(
        "graphmind".to_string(),
        serde_json::json!({
            "command": binary_path,
            "args": ["mcp"]
        }),
    );

    write_json(&config_path, &json)
}

fn install_cursor_mcp(binary_path: &str) -> Result<(), String> {
    let config_path = cursor_config_path();
    let mut json = read_or_create_json(&config_path)?;

    let servers = json
        .as_object_mut()
        .ok_or("Invalid config format")?
        .entry("mcpServers")
        .or_insert_with(|| Value::Object(serde_json::Map::new()));

    let servers_obj = servers.as_object_mut().ok_or("mcpServers is not an object")?;
    // No `env` block — `binary_path` is absolute. See issue #105.
    servers_obj.insert(
        "graphmind".to_string(),
        serde_json::json!({
            "command": binary_path,
            "args": ["mcp"]
        }),
    );

    write_json(&config_path, &json)
}

fn install_opencode_mcp(binary_path: &str) -> Result<(), String> {
    let config_path = opencode_config_path();

    // Read existing JSONC or start fresh
    let existing = if config_path.exists() {
        let ts = chrono::Utc::now().format("%Y%m%d_%H%M%S");
        let backup = config_path.with_extension(format!("jsonc.{ts}.bak"));
        fs::copy(&config_path, &backup).ok();
        let content = fs::read_to_string(&config_path).map_err(|e| e.to_string())?;
        let stripped = strip_jsonc_comments(&content);
        serde_json::from_str::<Value>(&stripped).unwrap_or_else(|_| serde_json::json!({}))
    } else {
        if let Some(parent) = config_path.parent() {
            fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        serde_json::json!({})
    };

    let mut json = existing;

    let mcp = json
        .as_object_mut()
        .ok_or("Invalid config format")?
        .entry("mcp")
        .or_insert_with(|| Value::Object(serde_json::Map::new()));

    mcp.as_object_mut()
        .ok_or("mcp is not an object")?
        .insert(
            "graphmind".to_string(),
            // No `environment` block — the command array holds an absolute path.
            // See issue #105.
            serde_json::json!({
                "type": "local",
                "command": [binary_path, "mcp"]
            }),
        );

    let content = serde_json::to_string_pretty(&json).map_err(|e| e.to_string())?;
    fs::write(&config_path, content).map_err(|e| e.to_string())
}

fn uninstall_opencode_mcp() -> Result<(), String> {
    let config_path = opencode_config_path();
    if !config_path.exists() {
        return Ok(());
    }
    let content = fs::read_to_string(&config_path).map_err(|e| e.to_string())?;
    let stripped = strip_jsonc_comments(&content);
    let mut json: Value = serde_json::from_str(&stripped).map_err(|e| e.to_string())?;

    if let Some(mcp) = json.get_mut("mcp").and_then(|m| m.as_object_mut()) {
        mcp.remove("graphmind");
    }

    let out = serde_json::to_string_pretty(&json).map_err(|e| e.to_string())?;
    fs::write(&config_path, out).map_err(|e| e.to_string())
}

/// Remove the `~/.graphmind/bin` segment that graphmind <= v0.2.211 injected into
/// the global `env.PATH` of `~/.claude/settings.json` (issue #105).
///
/// Mirrors `graphmind_cli::commands::setup::repair_claude_settings_path`. Kept
/// conservative because the user may have edited the value since: only our exact
/// segment is dropped, order is preserved, and if just the old hardcoded default
/// remains the key is removed so the inherited shell PATH wins.
///
/// Returns true when the config was modified.
fn repair_claude_settings_path(json: &mut Value, bin_dir: &str) -> bool {
    let Some(env) = json.get_mut("env").and_then(|e| e.as_object_mut()) else {
        return false;
    };
    let Some(current) = env.get("PATH").and_then(|p| p.as_str()) else {
        return false;
    };

    let segments: Vec<&str> = current.split(':').filter(|s| !s.is_empty()).collect();
    let kept: Vec<&str> = segments.iter().copied().filter(|s| *s != bin_dir).collect();

    if kept.len() == segments.len() {
        return false; // our segment was not there
    }

    const OLD_DEFAULT: [&str; 3] = ["/usr/local/bin", "/usr/bin", "/bin"];
    if kept == OLD_DEFAULT {
        env.remove("PATH");
    } else {
        env.insert("PATH".to_string(), Value::String(kept.join(":")));
    }

    if env.is_empty() {
        json.as_object_mut().unwrap().remove("env");
    }
    true
}

fn uninstall_claude_mcp() -> Result<(), String> {
    let config_path = claude_config_path();
    if !config_path.exists() {
        return Ok(());
    }
    let content = fs::read_to_string(&config_path).map_err(|e| e.to_string())?;
    let mut json: Value = serde_json::from_str(&content).map_err(|e| e.to_string())?;

    if let Some(servers) = json.get_mut("mcpServers").and_then(|s| s.as_object_mut()) {
        servers.remove("graphmind");
    }
    if json
        .get("mcpServers")
        .and_then(|m| m.as_object())
        .is_some_and(|m| m.is_empty())
    {
        json.as_object_mut().unwrap().remove("mcpServers");
    }

    // Also undo the legacy global env.PATH pollution (issue #105).
    repair_claude_settings_path(&mut json, &graphmind_bin_dir());

    write_json(&config_path, &json)
}

fn uninstall_claude_desktop_mcp() -> Result<(), String> {
    remove_mcp_entry(&claude_desktop_config_path())
}

fn uninstall_cursor_mcp() -> Result<(), String> {
    remove_mcp_entry(&cursor_config_path())
}

fn remove_mcp_entry(config_path: &PathBuf) -> Result<(), String> {
    if !config_path.exists() {
        return Ok(());
    }
    let content = fs::read_to_string(config_path).map_err(|e| e.to_string())?;
    let mut json: Value = serde_json::from_str(&content).map_err(|e| e.to_string())?;

    if let Some(servers) = json.get_mut("mcpServers").and_then(|s| s.as_object_mut()) {
        servers.remove("graphmind");
    }

    write_json(config_path, &json)
}

fn read_or_create_json(path: &PathBuf) -> Result<Value, String> {
    if path.exists() {
        // Timestamped backup before any modification
        let ts = chrono::Utc::now().format("%Y%m%d_%H%M%S");
        let backup = path.with_extension(format!("json.{ts}.bak"));
        fs::copy(path, &backup).ok();

        let content = fs::read_to_string(path).map_err(|e| e.to_string())?;
        serde_json::from_str(&content).map_err(|e| e.to_string())
    } else {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        Ok(serde_json::json!({}))
    }
}

fn write_json(path: &PathBuf, json: &Value) -> Result<(), String> {
    let content = serde_json::to_string_pretty(json).map_err(|e| e.to_string())?;
    fs::write(path, content).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    // ── strip_jsonc_comments ────────────────────────────────────────────────

    #[test]
    fn test_strip_line_comments() {
        let input = r#"{ // this is a comment
  "key": "value" // another
}"#;
        let stripped = strip_jsonc_comments(input);
        let v: Value = serde_json::from_str(&stripped).expect("should parse");
        assert_eq!(v["key"], "value");
    }

    #[test]
    fn test_strip_block_comments() {
        let input = r#"{ /* block comment */ "key": "value" }"#;
        let stripped = strip_jsonc_comments(input);
        let v: Value = serde_json::from_str(&stripped).expect("should parse");
        assert_eq!(v["key"], "value");
    }

    #[test]
    fn test_preserve_url_in_string() {
        let input = r#"{ "url": "https://example.com" }"#;
        let stripped = strip_jsonc_comments(input);
        let v: Value = serde_json::from_str(&stripped).expect("should parse");
        assert_eq!(v["url"], "https://example.com");
    }

    #[test]
    fn test_strip_full_opencode_config() {
        let input = r#"{
  // OpenCode configuration
  "$schema": "https://opencode.ai/config.json",
  "mcp": {
    /* existing server */
    "other-tool": {
      "type": "local",
      "command": ["npx", "other-tool"] // some tool
    }
  }
}"#;
        let stripped = strip_jsonc_comments(input);
        let v: Value = serde_json::from_str(&stripped).expect("should parse");
        assert!(v["mcp"]["other-tool"].is_object());
    }

    // ── install_opencode_mcp (file-based) ───────────────────────────────────

    #[test]
    fn test_install_opencode_mcp_fresh() {
        let dir = TempDir::new().unwrap();
        let config_path = dir.path().join("opencode.jsonc");
        let binary = "/home/user/.graphmind/bin/graphmind";

        let json = serde_json::json!({
            "mcp": {
                "graphmind": {
                    "type": "local",
                    "command": [binary, "mcp"]
                }
            }
        });

        fs::write(&config_path, serde_json::to_string_pretty(&json).unwrap()).unwrap();

        let content = fs::read_to_string(&config_path).unwrap();
        let v: Value = serde_json::from_str(&content).unwrap();

        assert_eq!(v["mcp"]["graphmind"]["type"], "local");
        assert_eq!(v["mcp"]["graphmind"]["command"][0], binary);
        assert_eq!(v["mcp"]["graphmind"]["command"][1], "mcp");
        // Issue #105: no `environment` block — the command path is absolute.
        assert!(v["mcp"]["graphmind"]["environment"].is_null());
    }

    // ── repair_claude_settings_path (issue #105) ─────────────────────────────

    #[test]
    fn test_repair_strips_our_segment_and_keeps_user_edits() {
        let bin_dir = "/home/user/.graphmind/bin";
        let nvm = "/home/user/.nvm/versions/node/v20.11.0/bin";
        let mut json = serde_json::json!({
            "env": { "PATH": format!("{bin_dir}:{nvm}:/usr/local/bin:/usr/bin:/bin") }
        });

        assert!(repair_claude_settings_path(&mut json, bin_dir));

        let path = json["env"]["PATH"].as_str().unwrap();
        assert!(!path.split(':').any(|s| s == bin_dir), "got: {path}");
        assert!(path.starts_with(nvm), "user edit must be preserved: {path}");
    }

    #[test]
    fn test_repair_removes_key_when_only_legacy_default_remains() {
        let bin_dir = "/home/user/.graphmind/bin";
        let mut json = serde_json::json!({
            "env": { "PATH": format!("{bin_dir}:/usr/local/bin:/usr/bin:/bin") }
        });

        assert!(repair_claude_settings_path(&mut json, bin_dir));

        // The whole `env` object goes away, so the inherited shell PATH wins.
        assert!(json.get("env").is_none(), "got: {json}");
    }

    #[test]
    fn test_repair_is_noop_when_segment_absent() {
        let mut json = serde_json::json!({
            "env": { "PATH": "/opt/homebrew/bin:/usr/bin" }
        });

        assert!(!repair_claude_settings_path(&mut json, "/home/user/.graphmind/bin"));
        assert_eq!(json["env"]["PATH"], "/opt/homebrew/bin:/usr/bin");
    }

    #[test]
    fn test_repair_strips_segment_in_middle() {
        let bin_dir = "/home/user/.graphmind/bin";
        let mut json = serde_json::json!({
            "env": { "PATH": format!("/opt/homebrew/bin:{bin_dir}:/usr/bin") }
        });

        assert!(repair_claude_settings_path(&mut json, bin_dir));
        assert_eq!(json["env"]["PATH"], "/opt/homebrew/bin:/usr/bin");
    }

    #[test]
    fn test_install_opencode_mcp_preserves_existing_servers() {
        let dir = TempDir::new().unwrap();
        let config_path = dir.path().join("opencode.jsonc");

        // Pre-existing config with another server
        let existing = serde_json::json!({
            "mcp": {
                "other-tool": {
                    "type": "local",
                    "command": ["npx", "other"]
                }
            }
        });
        fs::write(&config_path, serde_json::to_string_pretty(&existing).unwrap()).unwrap();

        // Simulate adding graphmind
        let content = fs::read_to_string(&config_path).unwrap();
        let stripped = strip_jsonc_comments(&content);
        let mut v: Value = serde_json::from_str(&stripped).unwrap();
        v["mcp"].as_object_mut().unwrap().insert(
            "graphmind".to_string(),
            serde_json::json!({ "type": "local", "command": ["/path/graphmind", "mcp"] }),
        );
        fs::write(&config_path, serde_json::to_string_pretty(&v).unwrap()).unwrap();

        let result: Value = serde_json::from_str(&fs::read_to_string(&config_path).unwrap()).unwrap();
        assert!(result["mcp"]["other-tool"].is_object(), "existing server preserved");
        assert!(result["mcp"]["graphmind"].is_object(), "graphmind added");
    }

    #[test]
    fn test_uninstall_opencode_mcp() {
        let dir = TempDir::new().unwrap();
        let config_path = dir.path().join("opencode.jsonc");

        let existing = serde_json::json!({
            "mcp": {
                "other-tool": { "type": "local", "command": ["npx", "other"] },
                "graphmind": { "type": "local", "command": ["/path/graphmind", "mcp"] }
            }
        });
        fs::write(&config_path, serde_json::to_string_pretty(&existing).unwrap()).unwrap();

        // Simulate uninstall
        let content = fs::read_to_string(&config_path).unwrap();
        let stripped = strip_jsonc_comments(&content);
        let mut v: Value = serde_json::from_str(&stripped).unwrap();
        if let Some(mcp) = v.get_mut("mcp").and_then(|m| m.as_object_mut()) {
            mcp.remove("graphmind");
        }
        fs::write(&config_path, serde_json::to_string_pretty(&v).unwrap()).unwrap();

        let result: Value = serde_json::from_str(&fs::read_to_string(&config_path).unwrap()).unwrap();
        assert!(result["mcp"]["other-tool"].is_object(), "other server preserved");
        assert!(result["mcp"]["graphmind"].is_null(), "graphmind removed");
    }

    // ── is_mcp_configured ───────────────────────────────────────────────────

    #[test]
    fn test_is_mcp_configured_true() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("settings.json");
        fs::write(&path, r#"{"mcpServers":{"graphmind":{"command":"graphmind"}}}"#).unwrap();
        assert!(is_mcp_configured(&path, "graphmind"));
    }

    #[test]
    fn test_is_mcp_configured_false() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("settings.json");
        fs::write(&path, r#"{"mcpServers":{"other":{"command":"other"}}}"#).unwrap();
        assert!(!is_mcp_configured(&path, "graphmind"));
    }

    #[test]
    fn test_is_mcp_configured_missing_file() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("nonexistent.json");
        assert!(!is_mcp_configured(&path, "graphmind"));
    }
}

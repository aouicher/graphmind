use crate::types::AiClient;
use serde_json::Value;
use std::fs;
use std::path::PathBuf;

fn home_dir() -> PathBuf {
    dirs::home_dir().expect("Cannot determine home directory")
}

fn get_cli_binary_path() -> String {
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
    home_dir().join(".cursor").exists()
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
    ]
}

#[tauri::command]
pub fn install_mcp_for_client(client_id: String) -> Result<(), String> {
    let binary_path = get_cli_binary_path();

    match client_id.as_str() {
        "claude-code" => install_claude_mcp(&binary_path),
        "claude-desktop" => install_claude_desktop_mcp(&binary_path),
        "cursor" => install_cursor_mcp(&binary_path),
        _ => Err(format!("Unsupported client: {client_id}")),
    }
}

#[tauri::command]
pub fn uninstall_mcp_for_client(client_id: String) -> Result<(), String> {
    match client_id.as_str() {
        "claude-code" => uninstall_claude_mcp(),
        "claude-desktop" => uninstall_claude_desktop_mcp(),
        "cursor" => uninstall_cursor_mcp(),
        _ => Err(format!("Unsupported client: {client_id}")),
    }
}

fn install_claude_mcp(binary_path: &str) -> Result<(), String> {
    let config_path = claude_config_path();
    let mut json = read_or_create_json(&config_path)?;

    let graphmind_bin_dir = home_dir().join(".graphmind").join("bin").to_string_lossy().to_string();

    // Inject ~/.graphmind/bin into env.PATH so Claude Code can resolve the binary
    {
        let env = json
            .as_object_mut()
            .ok_or("Invalid config format")?
            .entry("env")
            .or_insert_with(|| Value::Object(serde_json::Map::new()));
        let current_path = env
            .get("PATH")
            .and_then(|v| v.as_str())
            .unwrap_or("/usr/local/bin:/usr/bin:/bin")
            .to_string();
        if !current_path.contains(&graphmind_bin_dir) {
            let new_path = format!("{}:{}", graphmind_bin_dir, current_path);
            env.as_object_mut().unwrap().insert("PATH".to_string(), Value::String(new_path));
        }
    }

    let servers = json
        .as_object_mut()
        .ok_or("Invalid config format")?
        .entry("mcpServers")
        .or_insert_with(|| Value::Object(serde_json::Map::new()));

    let servers_obj = servers.as_object_mut().ok_or("mcpServers is not an object")?;
    servers_obj.insert(
        "graphmind".to_string(),
        serde_json::json!({
            "command": binary_path,
            "args": ["mcp"],
            "type": "stdio",
            "env": {
                "PATH": format!("{}:/usr/local/bin:/usr/bin:/bin", graphmind_bin_dir)
            }
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

fn uninstall_claude_mcp() -> Result<(), String> {
    remove_mcp_entry(&claude_config_path())
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
        // Backup before modifying
        let backup = path.with_extension("json.bak");
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

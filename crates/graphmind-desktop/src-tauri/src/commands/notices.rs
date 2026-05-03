use graphmind_config::{load_config, SETUP_VERSION};
use serde::Serialize;

#[derive(Serialize)]
pub struct SetupStatus {
    pub outdated: bool,
    pub local_version: u32,
    pub expected_version: u32,
}

#[derive(Serialize)]
pub struct Announcement {
    pub id: String,
    pub message: String,
    pub level: String,
    pub url: Option<String>,
}

#[tauri::command]
pub fn check_setup_status() -> SetupStatus {
    let config = load_config();
    SetupStatus {
        outdated: config.setup_version < SETUP_VERSION,
        local_version: config.setup_version,
        expected_version: SETUP_VERSION,
    }
}

#[tauri::command]
pub async fn run_setup() -> Result<String, String> {
    let cli_path = super::setup::get_cli_path();
    let output = std::process::Command::new(&cli_path)
        .arg("setup")
        .output()
        .map_err(|e| format!("Failed to run setup: {e}"))?;

    if output.status.success() {
        Ok("Setup complete".to_string())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        Err(format!("Setup failed: {stderr}"))
    }
}

#[tauri::command]
pub async fn check_announcements() -> Vec<Announcement> {
    let cache_path = graphmind_config::paths::graphmind_dir().join("announcements-cache.json");
    let dismissed_path = graphmind_config::paths::graphmind_dir().join("dismissed.json");

    // Read cache (fetched by CLI or by previous desktop run)
    let announcements: Vec<serde_json::Value> = if let Ok(raw) = std::fs::read_to_string(&cache_path) {
        serde_json::from_str::<serde_json::Value>(&raw)
            .ok()
            .and_then(|v| v.get("announcements").cloned())
            .and_then(|v| serde_json::from_value(v).ok())
            .unwrap_or_default()
    } else {
        Vec::new()
    };

    let dismissed: Vec<String> = if let Ok(raw) = std::fs::read_to_string(&dismissed_path) {
        serde_json::from_str::<serde_json::Value>(&raw)
            .ok()
            .and_then(|v| v.get("ids").cloned())
            .and_then(|v| serde_json::from_value(v).ok())
            .unwrap_or_default()
    } else {
        Vec::new()
    };

    let current_version = env!("CARGO_PKG_VERSION");

    announcements
        .into_iter()
        .filter_map(|a| {
            let id = a.get("id")?.as_str()?.to_string();
            if dismissed.contains(&id) {
                return None;
            }
            if let Some(min) = a.get("min_version").and_then(|v| v.as_str()) {
                if version_cmp(current_version, min) < 0 {
                    return None;
                }
            }
            if let Some(max) = a.get("max_version").and_then(|v| v.as_str()) {
                if version_cmp(current_version, max) > 0 {
                    return None;
                }
            }
            Some(Announcement {
                id,
                message: a.get("message")?.as_str()?.to_string(),
                level: a.get("level").and_then(|v| v.as_str()).unwrap_or("info").to_string(),
                url: a.get("url").and_then(|v| v.as_str()).map(String::from),
            })
        })
        .collect()
}

#[tauri::command]
pub fn dismiss_announcement(id: String) -> Result<(), String> {
    let path = graphmind_config::paths::graphmind_dir().join("dismissed.json");

    let mut dismissed: serde_json::Value = if let Ok(raw) = std::fs::read_to_string(&path) {
        serde_json::from_str(&raw).unwrap_or_else(|_| serde_json::json!({"ids": []}))
    } else {
        serde_json::json!({"ids": []})
    };

    if let Some(ids) = dismissed.get_mut("ids").and_then(|v| v.as_array_mut()) {
        let id_val = serde_json::Value::String(id);
        if !ids.contains(&id_val) {
            ids.push(id_val);
        }
    }

    let json = serde_json::to_string_pretty(&dismissed).map_err(|e| e.to_string())?;
    std::fs::write(&path, json).map_err(|e| e.to_string())?;
    Ok(())
}

fn version_cmp(a: &str, b: &str) -> i32 {
    let parse = |v: &str| -> Vec<u32> {
        v.split('.').filter_map(|s| s.parse().ok()).collect()
    };
    let va = parse(a);
    let vb = parse(b);
    for i in 0..3 {
        let x = va.get(i).unwrap_or(&0);
        let y = vb.get(i).unwrap_or(&0);
        if x < y { return -1; }
        if x > y { return 1; }
    }
    0
}

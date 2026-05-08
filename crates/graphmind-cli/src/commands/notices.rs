use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

const ANNOUNCEMENTS_URL: &str =
    "https://github.com/aouicher/graphmind-dist/releases/latest/download/announcements.json";
const LATEST_VERSION_URL: &str =
    "https://github.com/aouicher/graphmind-dist/releases/latest/download/latest.json";
const CACHE_TTL_SECS: u64 = 86400;
const FETCH_TIMEOUT_SECS: u32 = 2;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Announcement {
    pub id: String,
    pub message: String,
    #[serde(default = "default_level")]
    pub level: String,
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default)]
    pub min_version: Option<String>,
    #[serde(default)]
    pub max_version: Option<String>,
}

fn default_level() -> String {
    "info".to_string()
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct AnnouncementsCache {
    fetched_at: u64,
    announcements: Vec<Announcement>,
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct DismissedList {
    ids: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct UpdateCache {
    fetched_at: u64,
    latest_version: String,
}

fn update_cache_path() -> PathBuf {
    graphmind_config::paths::graphmind_dir().join("update-check.json")
}

pub fn check_cli_update() {
    // Skip if stdout is not a TTY (MCP mode, pipes, etc.)
    if !std::io::IsTerminal::is_terminal(&std::io::stderr()) {
        return;
    }

    let current = env!("CARGO_PKG_VERSION");
    let Some(latest) = fetch_latest_version() else { return };

    if version_cmp(&latest, current) > 0 {
        eprintln!(
            "\x1b[33m[graphmind]\x1b[0m Update available: {} → {} — run \x1b[36mgraphmind update\x1b[0m",
            current, latest
        );
    }
}

fn fetch_latest_version() -> Option<String> {
    let path = update_cache_path();
    let now = now_secs();

    if let Ok(raw) = fs::read_to_string(&path) {
        if let Ok(cache) = serde_json::from_str::<UpdateCache>(&raw) {
            if now - cache.fetched_at < CACHE_TTL_SECS && !cache.latest_version.is_empty() {
                return Some(cache.latest_version);
            }
        }
    }

    let output = std::process::Command::new("curl")
        .args([
            "-fsSL",
            "--max-time",
            &FETCH_TIMEOUT_SECS.to_string(),
            LATEST_VERSION_URL,
        ])
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let body = String::from_utf8_lossy(&output.stdout);
    // latest.json has a "version" field (same format as Tauri updater manifest)
    let latest = serde_json::from_str::<serde_json::Value>(&body)
        .ok()
        .and_then(|v| v.get("version").and_then(|v| v.as_str()).map(|s| s.to_string()))?;
    if latest.is_empty() {
        return None;
    }

    let cache = UpdateCache { fetched_at: now, latest_version: latest.clone() };
    let json = serde_json::to_string(&cache).unwrap_or_default();
    fs::write(&path, json).ok();

    Some(latest)
}

fn cache_path() -> PathBuf {
    graphmind_config::paths::graphmind_dir().join("announcements-cache.json")
}

fn dismissed_path() -> PathBuf {
    graphmind_config::paths::graphmind_dir().join("dismissed.json")
}

fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

pub fn check_setup_version() {
    let config = graphmind_config::load_config();
    let local = config.setup_version;
    let expected = graphmind_config::SETUP_VERSION;
    if local < expected {
        eprintln!(
            "\x1b[33m[graphmind]\x1b[0m Hooks/skills outdated (v{} → v{}). Run `graphmind setup` to update.",
            local, expected
        );
    }
}

pub fn check_announcements() {
    let cache = load_or_fetch_cache();
    let Some(cache) = cache else { return };

    let dismissed = load_dismissed();
    let current_version = env!("CARGO_PKG_VERSION");

    let announcement = cache
        .announcements
        .iter()
        .find(|a| {
            if dismissed.ids.contains(&a.id) {
                return false;
            }
            if let Some(ref min) = a.min_version {
                if version_cmp(current_version, min) < 0 {
                    return false;
                }
            }
            if let Some(ref max) = a.max_version {
                if version_cmp(current_version, max) > 0 {
                    return false;
                }
            }
            true
        });

    if let Some(a) = announcement {
        let color = match a.level.as_str() {
            "breaking" => "\x1b[31m",
            "warning" => "\x1b[33m",
            _ => "\x1b[36m",
        };
        let label = match a.level.as_str() {
            "breaking" => "BREAKING",
            "warning" => "notice",
            _ => "info",
        };
        eprint!("{}[graphmind {label}]\x1b[0m {}", color, a.message);
        if let Some(ref url) = a.url {
            eprint!(" → {url}");
        }
        eprintln!();
    }
}

#[allow(dead_code)]
pub fn dismiss(id: &str) {
    let mut dismissed = load_dismissed();
    if !dismissed.ids.contains(&id.to_string()) {
        dismissed.ids.push(id.to_string());
        let json = serde_json::to_string_pretty(&dismissed).unwrap_or_default();
        fs::write(dismissed_path(), json).ok();
    }
}

pub fn check_schema_version() {
    if !std::io::IsTerminal::is_terminal(&std::io::stderr()) {
        return;
    }
    let projects = graphmind_config::Registry::list();
    let stale_count = projects.iter().filter(|p| {
        let db_path = graphmind_config::paths::graph_db_path(&p.slug);
        graphmind_db::schema::schema_needs_reset(db_path.to_str().unwrap_or(""))
    }).count();
    if stale_count > 0 {
        eprintln!(
            "\x1b[33m[graphmind]\x1b[0m {} project(s) have an outdated graph index. Run \x1b[36mgraphmind build --reset --all\x1b[0m to reindex.",
            stale_count
        );
    }
}

#[allow(dead_code)]
pub fn get_active_announcements() -> Vec<Announcement> {
    let cache = load_or_fetch_cache();
    let Some(cache) = cache else {
        return Vec::new();
    };

    let dismissed = load_dismissed();
    let current_version = env!("CARGO_PKG_VERSION");

    cache
        .announcements
        .into_iter()
        .filter(|a| {
            if dismissed.ids.contains(&a.id) {
                return false;
            }
            if let Some(ref min) = a.min_version {
                if version_cmp(current_version, min) < 0 {
                    return false;
                }
            }
            if let Some(ref max) = a.max_version {
                if version_cmp(current_version, max) > 0 {
                    return false;
                }
            }
            true
        })
        .collect()
}

fn load_or_fetch_cache() -> Option<AnnouncementsCache> {
    let path = cache_path();
    let now = now_secs();

    if let Ok(raw) = fs::read_to_string(&path) {
        if let Ok(cache) = serde_json::from_str::<AnnouncementsCache>(&raw) {
            if now - cache.fetched_at < CACHE_TTL_SECS {
                return Some(cache);
            }
        }
    }

    // Fetch in foreground with short timeout — non-blocking failure
    let output = std::process::Command::new("curl")
        .args([
            "-fsSL",
            "--max-time",
            &FETCH_TIMEOUT_SECS.to_string(),
            ANNOUNCEMENTS_URL,
        ])
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let body = String::from_utf8_lossy(&output.stdout);
    let announcements: Vec<Announcement> = serde_json::from_str(&body).ok()?;

    let cache = AnnouncementsCache {
        fetched_at: now,
        announcements,
    };

    let json = serde_json::to_string(&cache).unwrap_or_default();
    fs::write(&path, json).ok();

    Some(cache)
}

fn load_dismissed() -> DismissedList {
    let path = dismissed_path();
    if let Ok(raw) = fs::read_to_string(path) {
        serde_json::from_str(&raw).unwrap_or_default()
    } else {
        DismissedList::default()
    }
}

fn version_cmp(a: &str, b: &str) -> i32 {
    let parse = |v: &str| -> Vec<u32> {
        v.split('.')
            .filter_map(|s| s.parse().ok())
            .collect()
    };
    let va = parse(a);
    let vb = parse(b);
    for i in 0..3 {
        let x = va.get(i).unwrap_or(&0);
        let y = vb.get(i).unwrap_or(&0);
        if x < y {
            return -1;
        }
        if x > y {
            return 1;
        }
    }
    0
}

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

const ANNOUNCEMENTS_URL: &str =
    "https://github.com/aouicher/graphmind/releases/latest/download/announcements.json";
const CACHE_TTL_SECS: u64 = 86400;
const FETCH_TIMEOUT_SECS: u64 = 2;

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

pub fn check_cli_update() {
    let current = env!("CARGO_PKG_VERSION");
    let Some(latest) = graphmind_config::releases::check_latest_version(FETCH_TIMEOUT_SECS) else { return };

    if version_cmp(&latest, current) > 0 {
        eprintln!(
            "\x1b[33m[graphmind]\x1b[0m Update available: {} → {} — run \x1b[36mgraphmind update\x1b[0m",
            current, latest
        );
    }
}

/// Cache files are read by every interactive command, so they are replaced, never truncated.
fn write_cache(path: &std::path::Path, cache: &impl Serialize) {
    if let Ok(json) = serde_json::to_string(cache) {
        graphmind_config::write_atomic(path, json.as_bytes()).ok();
    }
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

/// Auto-reinstall hooks/skills/CLAUDE.md in background when binary version is newer than installed setup.
/// Runs at most once per day (stamp file). Triggered by brew upgrade or any binary replacement.
///
/// `setup` runs as a detached, silent child process: a thread would die with the command that
/// started it (usually a few milliseconds later), and its output must not reach that command's
/// stdout (hooks capture it). The stamp is written before spawning, so the many commands hooks
/// run concurrently start one setup, not one each.
pub fn auto_setup_if_outdated() {
    // An unreadable config.json would read as setup_version 0: not a user who is out of date.
    let Ok(config) = graphmind_config::read_config() else { return };
    // Installed Claude Code hook scripts that differ from this binary's also need a setup, so a
    // hook change doesn't depend on a SETUP_VERSION bump.
    if config.setup_version >= graphmind_config::SETUP_VERSION && !super::claude_hook::installed_hooks_outdated() {
        return;
    }

    // "<unix ts> <setup version> <hooks fingerprint>": throttles retries of this version's setup
    // only. A stamp left by an older version (plain timestamp, or no fingerprint) must not delay
    // the new one by a day.
    const ONE_DAY: u64 = 86_400;
    let expected = graphmind_config::SETUP_VERSION;
    let hooks = super::claude_hook::hooks_fingerprint();
    let stamp = graphmind_config::paths::graphmind_dir().join("auto-setup.stamp");
    let now = now_secs();
    if let Ok(raw) = fs::read_to_string(&stamp) {
        let mut parts = raw.split_whitespace();
        let ts = parts.next().and_then(|t| t.parse::<u64>().ok());
        let version = parts.next().and_then(|v| v.parse::<u32>().ok());
        let fingerprint = parts.next().and_then(|f| f.parse::<u64>().ok());
        if let (Some(ts), Some(version), Some(fingerprint)) = (ts, version, fingerprint) {
            if version == expected && fingerprint == hooks && now.saturating_sub(ts) < ONE_DAY {
                return;
            }
        }
    }
    if fs::write(&stamp, format!("{now} {expected} {hooks}")).is_err() {
        return;
    }

    let Ok(exe) = std::env::current_exe() else {
        return;
    };
    let mut cmd = std::process::Command::new(exe);
    cmd.arg("setup")
        .env("GRAPHMIND_SKIP_NOTICES", "1")
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    let _ = cmd.spawn();
}

pub fn check_setup_version() {
    // "Run graphmind setup" is the wrong advice when config.json can't be read.
    let Ok(config) = graphmind_config::read_config() else { return };
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
            if now.saturating_sub(cache.fetched_at) < CACHE_TTL_SECS {
                return Some(cache);
            }
        }
    }
    // Record the attempt first (empty list), so a failed fetch isn't retried on every command.
    write_cache(&path, &AnnouncementsCache { fetched_at: now, announcements: Vec::new() });

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
    write_cache(&path, &cache);

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

fn memory_clean_stamp_path() -> PathBuf {
    graphmind_config::paths::graphmind_dir().join("memory-clean.stamp")
}

/// Run memory clean (expire, commit purge, dedup, auto-promote) at most once per day, inline
/// and silent. Not on a background thread: it would die with short commands before writing its
/// stamp, so the clean restarted on the next command, and it could write back entries the
/// command itself had just deleted. The stamp is written first, so concurrent commands (hooks
/// run several at once) start one clean, not one each.
pub fn memory_auto_clean() {
    const ONE_DAY: u64 = 86_400;
    let stamp = memory_clean_stamp_path();
    let now = now_secs();

    // Throttle: skip if already ran within the last 24h
    if let Ok(raw) = fs::read_to_string(&stamp) {
        if let Ok(ts) = raw.trim().parse::<u64>() {
            if now.saturating_sub(ts) < ONE_DAY {
                return;
            }
        }
    }

    if graphmind_config::write_atomic(&stamp, now.to_string().as_bytes()).is_err() {
        return;
    }
    super::memory::run_consolidate(None, false);
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

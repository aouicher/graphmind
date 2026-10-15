//! The latest graphmind release on GitHub, and the cache that keeps commands from asking for it
//! more than once a day (`~/.graphmind/update-check.json`, shared by the CLI and the MCP server).

use crate::{paths, write_atomic};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

const LATEST_RELEASE_URL: &str = "https://api.github.com/repos/aouicher/graphmind/releases/latest";
const CHECK_INTERVAL_SECS: u64 = 86_400;

/// The latest release's version without its `v` (`0.3.6`), asked of GitHub now.
pub fn fetch_latest_version(timeout_secs: u64) -> Result<String, String> {
    let output = Command::new("curl")
        .args(["-fsSL", "--max-time", &timeout_secs.to_string()])
        .args(["-H", "Accept: application/vnd.github+json", LATEST_RELEASE_URL])
        .output()
        .map_err(|e| format!("Failed to check for updates: {e}"))?;
    if !output.status.success() {
        return Err("Failed to fetch latest release info".to_string());
    }
    parse_latest_version(&output.stdout).ok_or_else(|| "Could not parse release tag".to_string())
}

/// The latest release's version, asked of GitHub at most once a day. A failed check counts too,
/// so an offline machine waits for the timeout once a day, not on every command.
pub fn check_latest_version(timeout_secs: u64) -> Option<String> {
    let now = now_secs();
    if let Some(check) = read_check().filter(|check| check.is_recent(now)) {
        return check.version();
    }
    // Recorded before asking, so a check that fails or hangs isn't retried by the next command.
    write_check(&LastCheck { fetched_at: now, latest_version: String::new() });
    let latest = fetch_latest_version(timeout_secs).ok()?;
    write_check(&LastCheck { fetched_at: now, latest_version: latest.clone() });
    Some(latest)
}

/// The latest release's version found by the last check, if it is less than a day old and
/// succeeded. Never asks GitHub.
pub fn last_known_latest_version() -> Option<String> {
    read_check().filter(|check| check.is_recent(now_secs()))?.version()
}

/// The last check, as stored: `latest_version` is empty when it failed.
#[derive(Serialize, Deserialize)]
struct LastCheck {
    fetched_at: u64,
    latest_version: String,
}

impl LastCheck {
    fn is_recent(&self, now: u64) -> bool {
        now.saturating_sub(self.fetched_at) < CHECK_INTERVAL_SECS
    }

    fn version(self) -> Option<String> {
        (!self.latest_version.is_empty()).then_some(self.latest_version)
    }
}

fn parse_latest_version(body: &[u8]) -> Option<String> {
    let release: serde_json::Value = serde_json::from_slice(body).ok()?;
    let version = release.get("tag_name")?.as_str()?.trim_start_matches('v');
    (!version.is_empty()).then(|| version.to_string())
}

fn check_path() -> PathBuf {
    paths::graphmind_dir().join("update-check.json")
}

fn read_check() -> Option<LastCheck> {
    serde_json::from_str(&std::fs::read_to_string(check_path()).ok()?).ok()
}

/// Replaced, never truncated: every interactive command and the MCP server read it.
fn write_check(check: &LastCheck) {
    if let Ok(json) = serde_json::to_string(check) {
        write_atomic(&check_path(), json.as_bytes()).ok();
    }
}

fn now_secs() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_version_is_the_tag_without_its_v() {
        assert_eq!(parse_latest_version(br#"{"tag_name":"v0.3.6","name":"v0.3.6"}"#).as_deref(), Some("0.3.6"));
        assert_eq!(parse_latest_version(br#"{"tag_name":"1.0.0"}"#).as_deref(), Some("1.0.0"));
    }

    #[test]
    fn a_body_without_a_tag_has_no_version() {
        for body in [&br#"{"message":"API rate limit exceeded"}"#[..], br#"{"tag_name":""}"#, br#"{"tag_name":"v"}"#, b"<html>", b""] {
            assert_eq!(parse_latest_version(body), None, "{}", String::from_utf8_lossy(body));
        }
    }

    #[test]
    fn a_check_is_recent_for_a_day_and_a_failed_one_has_no_version() {
        let check = |fetched_at, latest_version: &str| LastCheck { fetched_at, latest_version: latest_version.to_string() };
        let now = 1_000_000;
        assert!(check(now - 10, "0.3.6").is_recent(now));
        assert!(!check(now - CHECK_INTERVAL_SECS, "0.3.6").is_recent(now));
        // A check dated in the future (the clock was set back) counts as recent, without overflow.
        assert!(check(now + 10, "0.3.6").is_recent(now));
        assert_eq!(check(now, "0.3.6").version().as_deref(), Some("0.3.6"));
        assert_eq!(check(now, "").version(), None);
    }
}

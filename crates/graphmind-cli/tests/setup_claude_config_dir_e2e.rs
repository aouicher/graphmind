//! E2E tests for `CLAUDE_CONFIG_DIR`: where Claude Code keeps ~/.claude.json, the folder setup
//! records for processes that don't inherit the variable (the desktop app), and the `claude` CLI
//! writing to that folder.
//!
//! Run with:
//!   cargo test -p graphmind-cli --test setup_claude_config_dir_e2e -- --test-threads=1

use common::{claude_code_step, fake_claude, with_home, FAKE_CLAUDE_OK};
use graphmind_cli::commands::setup::{install_mcp_client, migrate_project_mcp_entries, setup, McpClient, Outcome};
use graphmind_config::mcp_clients::{sync_claude_config_dir, RecordedConfigDir};
use serde_json::Value;
use std::fs;

mod common;

// ---------------------------------------------------------------------------
// 13. CLAUDE_CONFIG_DIR — where Claude Code (and `claude mcp add`) keeps ~/.claude.json then
// ---------------------------------------------------------------------------

#[test]
fn claude_code_entry_follows_claude_config_dir() {
    with_home(|home| {
        let config_dir = home.join("claude-config");
        fs::create_dir_all(&config_dir).unwrap();
        let claude_json = config_dir.join(".claude.json");
        fs::write(
            &claude_json,
            r#"{"projects": {"/repo/a": {"mcpServers": {"graphmind": {"command": "/old/graphmind"}}}}}"#,
        )
        .unwrap();
        unsafe { std::env::set_var("CLAUDE_CONFIG_DIR", &config_dir) };

        let outcome = claude_code_step();
        migrate_project_mcp_entries(outcome.is_in_place());
        unsafe { std::env::remove_var("CLAUDE_CONFIG_DIR") };

        assert_eq!(outcome, Outcome::Written);
        let json: Value = serde_json::from_str(&fs::read_to_string(&claude_json).unwrap()).unwrap();
        assert!(json["mcpServers"]["graphmind"]["command"].is_string(), "{json}");
        assert!(json["projects"]["/repo/a"]["mcpServers"]["graphmind"].is_null(), "migrated in the same file: {json}");
        assert!(!home.join(".claude.json").exists(), "no second entry in ~/.claude.json");
    });
}

/// Setup without CLAUDE_CONFIG_DIR (the desktop app runs it so) uses the folder an earlier setup
/// recorded, keeps it while it exists and drops it once it's gone.
#[test]
fn setup_records_claude_config_dir_for_processes_without_it() {
    with_home(|home| {
        let recorded = || graphmind_config::read_config().unwrap().claude_config_dir;
        let config_dir = home.join("claude-config");
        fs::create_dir_all(&config_dir).unwrap();
        unsafe { std::env::set_var("CLAUDE_CONFIG_DIR", &config_dir) };
        setup();
        unsafe { std::env::remove_var("CLAUDE_CONFIG_DIR") };
        assert_eq!(recorded(), Some(config_dir.clone()));

        assert_eq!(McpClient::ClaudeCodeUser.config_path(), config_dir.join(".claude.json"));
        assert_eq!(install_mcp_client(&McpClient::ClaudeCodeUser), Outcome::Unchanged);
        setup();
        assert_eq!(recorded(), Some(config_dir.clone()), "kept while the folder exists");
        assert!(!home.join(".claude.json").exists(), "no second entry in ~/.claude.json");

        fs::remove_dir_all(&config_dir).unwrap();
        setup();
        assert_eq!(recorded(), None, "dropped once the folder is gone");
        let json: Value = serde_json::from_str(&fs::read_to_string(home.join(".claude.json")).unwrap()).unwrap();
        assert!(json["mcpServers"]["graphmind"]["command"].is_string(), "{json}");
    });
}

/// `CLAUDE_CONFIG_DIR=` (set but empty) means ~/.claude.json, so it drops a recorded folder
/// that still exists.
#[test]
fn empty_claude_config_dir_drops_the_recorded_one() {
    with_home(|home| {
        let config_dir = home.join("claude-config");
        fs::create_dir_all(&config_dir).unwrap();
        unsafe { std::env::set_var("CLAUDE_CONFIG_DIR", &config_dir) };
        assert_eq!(sync_claude_config_dir(), Ok(RecordedConfigDir::Recorded(config_dir.clone())));
        assert_eq!(sync_claude_config_dir(), Ok(RecordedConfigDir::Unchanged));

        unsafe { std::env::set_var("CLAUDE_CONFIG_DIR", "") };
        let cleared = sync_claude_config_dir();
        let path = McpClient::ClaudeCodeUser.config_path();
        unsafe { std::env::remove_var("CLAUDE_CONFIG_DIR") };
        assert_eq!(cleared, Ok(RecordedConfigDir::Cleared(config_dir)));
        assert_eq!(path, home.join(".claude.json"));
        assert_eq!(McpClient::ClaudeCodeUser.config_path(), home.join(".claude.json"));
    });
}

/// Recording the folder goes through the same guard as every config save (#113).
#[test]
fn unreadable_config_json_is_not_saved_over_to_record_claude_config_dir() {
    with_home(|home| {
        let config_json = home.join(".graphmind").join("config.json");
        fs::create_dir_all(config_json.parent().unwrap()).unwrap();
        fs::write(&config_json, r#"{"projects": {"#).unwrap();
        unsafe { std::env::set_var("CLAUDE_CONFIG_DIR", home.join("claude-config")) };
        let result = sync_claude_config_dir();
        unsafe { std::env::remove_var("CLAUDE_CONFIG_DIR") };
        assert!(result.is_err(), "{result:?}");
        assert_eq!(fs::read_to_string(&config_json).unwrap(), r#"{"projects": {"#);
    });
}

/// The `claude` CLI gets the recorded folder as CLAUDE_CONFIG_DIR, so it writes the file setup
/// checks instead of ~/.claude.json (which would also need a direct write as fallback).
#[cfg(unix)]
#[test]
fn claude_cli_writes_to_the_recorded_config_dir() {
    with_home(|home| {
        let config_dir = home.join("claude-config");
        fs::create_dir_all(&config_dir).unwrap();
        unsafe { std::env::set_var("CLAUDE_CONFIG_DIR", &config_dir) };
        sync_claude_config_dir().unwrap();
        unsafe { std::env::remove_var("CLAUDE_CONFIG_DIR") };
        fake_claude(home, FAKE_CLAUDE_OK);

        assert_eq!(install_mcp_client(&McpClient::ClaudeCodeUser), Outcome::Written);
        assert!(fs::read_to_string(home.join("claude-calls.log")).unwrap().contains("mcp add -s user graphmind"));
        assert!(config_dir.join(".claude.json").exists());
        assert!(!home.join(".claude.json").exists(), "the CLI wrote another file than the one checked");
    });
}

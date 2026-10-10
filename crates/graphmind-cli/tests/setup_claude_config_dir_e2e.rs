//! E2E tests for `CLAUDE_CONFIG_DIR`: where Claude Code keeps ~/.claude.json, the folder setup
//! records for the desktop app (which doesn't inherit the variable), and the `claude` CLI writing
//! to that folder.
//!
//! Run with:
//!   cargo test -p graphmind-cli --test setup_claude_config_dir_e2e -- --test-threads=1

use common::{claude_code_step, fake_claude, graphmind_in, with_home, FAKE_CLAUDE_OK};
use graphmind_cli::commands::setup::{install_mcp_client, migrate_project_mcp_entries, setup, McpClient, Outcome};
use graphmind_config::mcp_clients::{inherit_recorded_claude_config_dir, sync_claude_config_dir, RecordedConfigDir};
use serde_json::Value;
use std::fs;
use std::path::Path;

mod common;

fn read_json(path: &Path) -> Value {
    serde_json::from_str(&fs::read_to_string(path).unwrap()).unwrap()
}

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
        let json = read_json(&claude_json);
        assert!(json["mcpServers"]["graphmind"]["command"].is_string(), "{json}");
        assert!(json["projects"]["/repo/a"]["mcpServers"]["graphmind"].is_null(), "migrated in the same file: {json}");
        assert!(!home.join(".claude.json").exists(), "no second entry in ~/.claude.json");
    });
}

/// The step-8 cleanup covers ~/.claude.json too: Claude Code reads it whenever it runs without
/// CLAUDE_CONFIG_DIR, so a stale project-scope entry there would still win for that project.
#[test]
fn project_entry_cleanup_covers_home_and_claude_config_dir() {
    with_home(|home| {
        let config_dir = home.join("claude-config");
        fs::create_dir_all(&config_dir).unwrap();
        let stale = r#"{"projects": {"/repo/a": {"mcpServers": {"graphmind": {"command": "/old/graphmind"}, "other": {"command": "other"}}}}}"#;
        let files = [config_dir.join(".claude.json"), home.join(".claude.json")];
        for file in &files {
            fs::write(file, stale).unwrap();
        }
        unsafe { std::env::set_var("CLAUDE_CONFIG_DIR", &config_dir) };
        let outcome = install_mcp_client(&McpClient::claude_code());
        migrate_project_mcp_entries(outcome.is_in_place());
        unsafe { std::env::remove_var("CLAUDE_CONFIG_DIR") };

        assert_eq!(outcome, Outcome::Written);
        for file in &files {
            let json = read_json(file);
            let servers = &json["projects"]["/repo/a"]["mcpServers"];
            assert!(servers["graphmind"].is_null(), "{}: {json}", file.display());
            assert_eq!(servers["other"]["command"], "other", "{}: {json}", file.display());
        }
    });
}

/// Only the desktop app uses the folder setup recorded, by setting CLAUDE_CONFIG_DIR from it at
/// startup. Setup in a shell without the variable configures ~/.claude.json and keeps the record
/// while the folder exists; it drops it once the folder is gone.
#[test]
fn recorded_claude_config_dir_is_for_the_desktop_only() {
    with_home(|home| {
        let recorded = || graphmind_config::read_config().unwrap().claude_config_dir;
        let config_dir = home.join("claude-config");
        fs::create_dir_all(&config_dir).unwrap();
        unsafe { std::env::set_var("CLAUDE_CONFIG_DIR", &config_dir) };
        setup();
        unsafe { std::env::remove_var("CLAUDE_CONFIG_DIR") };
        assert_eq!(recorded(), Some(config_dir.clone()));
        assert!(config_dir.join(".claude.json").exists());

        assert_eq!(McpClient::claude_code().config_path(), home.join(".claude.json"), "the CLI ignores the record");
        assert_eq!(sync_claude_config_dir(), Ok(RecordedConfigDir::Kept(config_dir.clone())));
        setup();
        assert_eq!(recorded(), Some(config_dir.clone()), "kept for the desktop app while the folder exists");
        let json = read_json(&home.join(".claude.json"));
        assert!(json["mcpServers"]["graphmind"]["command"].is_string(), "the CLI configures ~/.claude.json: {json}");

        // The desktop app at startup.
        assert_eq!(inherit_recorded_claude_config_dir(), Some(config_dir.clone()));
        let desktop = McpClient::claude_code().config_path();
        unsafe { std::env::remove_var("CLAUDE_CONFIG_DIR") };
        assert_eq!(desktop, config_dir.join(".claude.json"));

        fs::remove_dir_all(&config_dir).unwrap();
        setup();
        assert_eq!(recorded(), None, "dropped once the folder is gone");
        assert_eq!(inherit_recorded_claude_config_dir(), None);
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
        let path = McpClient::claude_code().config_path();
        unsafe { std::env::remove_var("CLAUDE_CONFIG_DIR") };
        assert_eq!(cleared, Ok(RecordedConfigDir::Cleared(config_dir)));
        assert_eq!(path, home.join(".claude.json"));
    });
}

/// A relative CLAUDE_CONFIG_DIR is ignored, and setup says so. Run as a command in the temp HOME,
/// so a regression writes there, not into the checkout.
#[test]
fn relative_claude_config_dir_is_ignored_and_reported() {
    let home = tempfile::tempdir().unwrap();
    let home = home.path();
    let out = graphmind_in(Path::new(env!("CARGO_BIN_EXE_graphmind")), home, "/bin/zsh")
        .arg("setup")
        .env("CLAUDE_CONFIG_DIR", "relative-claude")
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(out.status.success(), "{stdout}\n{}", String::from_utf8_lossy(&out.stderr));

    assert!(stdout.contains("CLAUDE_CONFIG_DIR=relative-claude is relative, ignored"), "{stdout}");
    assert!(read_json(&home.join(".claude.json"))["mcpServers"]["graphmind"].is_object());
    assert!(!home.join("relative-claude").exists(), "setup wrote into the relative folder");
    let config = read_json(&home.join(".graphmind/config.json"));
    assert!(config["claude_config_dir"].is_null(), "a relative folder is never recorded: {config}");
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

/// The `claude` CLI gets the client's folder as CLAUDE_CONFIG_DIR, so it writes the file graphmind
/// checks: here the desktop app's, set from the record.
#[cfg(unix)]
#[test]
fn claude_cli_writes_to_the_config_dir_the_desktop_inherits() {
    with_home(|home| {
        let config_dir = home.join("claude-config");
        fs::create_dir_all(&config_dir).unwrap();
        unsafe { std::env::set_var("CLAUDE_CONFIG_DIR", &config_dir) };
        sync_claude_config_dir().unwrap();
        unsafe { std::env::remove_var("CLAUDE_CONFIG_DIR") };
        fake_claude(home, FAKE_CLAUDE_OK);

        inherit_recorded_claude_config_dir();
        let outcome = install_mcp_client(&McpClient::claude_code());
        unsafe { std::env::remove_var("CLAUDE_CONFIG_DIR") };

        assert_eq!(outcome, Outcome::Written);
        assert!(fs::read_to_string(home.join("claude-calls.log")).unwrap().contains("mcp add -s user graphmind"));
        assert!(config_dir.join(".claude.json").exists());
        assert!(!home.join(".claude.json").exists(), "the CLI wrote another file than the one checked");
    });
}

/// For ~/.claude.json the `claude` CLI runs without CLAUDE_CONFIG_DIR, so `CLAUDE_CONFIG_DIR=
/// graphmind setup` doesn't depend on how `claude` reads an empty value.
#[cfg(unix)]
#[test]
fn claude_cli_runs_without_an_empty_claude_config_dir() {
    with_home(|home| {
        fake_claude(home, &format!("echo \"${{CLAUDE_CONFIG_DIR-unset}}\" >> \"$HOME/claude-env.log\"\n{FAKE_CLAUDE_OK}"));
        unsafe { std::env::set_var("CLAUDE_CONFIG_DIR", "") };
        let outcome = install_mcp_client(&McpClient::claude_code());
        unsafe { std::env::remove_var("CLAUDE_CONFIG_DIR") };

        assert_eq!(outcome, Outcome::Written);
        let seen = fs::read_to_string(home.join("claude-env.log")).unwrap();
        assert!(seen.lines().all(|line| line == "unset"), "claude saw CLAUDE_CONFIG_DIR: {seen:?}");
    });
}

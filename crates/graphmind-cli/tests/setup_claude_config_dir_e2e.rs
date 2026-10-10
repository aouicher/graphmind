//! E2E tests for `CLAUDE_CONFIG_DIR`: where Claude Code keeps ~/.claude.json, the folder setup
//! records for the desktop app (which doesn't inherit the variable), the `claude` CLI writing to
//! that folder, and one setup per Claude Code folder.
//!
//! Run with:
//!   cargo test -p graphmind-cli --test setup_claude_config_dir_e2e -- --test-threads=1

use common::{claude_code_step, fake_claude, graphmind_in, in_config_dir, with_home, FAKE_CLAUDE_OK};
use graphmind_cli::commands::setup::{install_mcp_client, migrate_project_mcp_entries, setup, McpClient, Outcome};
use graphmind_config::claude_home::{inherit_recorded_claude_config_dir, sync_claude_config_dir, ClaudeHome, RecordedConfigDir};
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
    assert!(home.join(".claude/settings.json").exists(), "~/.claude not set up");
    assert!(read_json(&home.join(".claude.json"))["mcpServers"]["graphmind"].is_object());
    assert!(!home.join("relative-claude").exists(), "setup wrote into the relative folder");
    let config = read_json(&home.join(".graphmind/config.json"));
    assert!(config["claude_config_dir"].is_null(), "a relative folder is never recorded: {config}");
    assert!(config["setup_versions"].is_null(), "a relative folder has no setup state: {config}");
    assert_eq!(config["setup_version"], graphmind_config::SETUP_VERSION, "~/.claude marked set up");
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

// ---------------------------------------------------------------------------
// 16. One setup per Claude Code folder (CLAUDE_CONFIG_DIR)
// ---------------------------------------------------------------------------

/// The setup version auto-setup sees for `dir` (`None`: ~/.claude).
fn setup_version_of(dir: Option<&Path>) -> u32 {
    in_config_dir(dir, || ClaudeHome::current().setup_version(&graphmind_config::read_config().unwrap()))
}

/// Auto-setup's throttle stamp for a CLAUDE_CONFIG_DIR folder.
fn stamp_of(home: &Path, dir: &Path) -> std::path::PathBuf {
    let id = graphmind_config::fnv1a(dir.to_string_lossy().bytes());
    home.join(format!(".graphmind/auto-setup-{id:016x}.stamp"))
}

/// Hooks (scripts and registration), skills, the CLAUDE.md block and the MCP entry all go to the
/// CLAUDE_CONFIG_DIR folder; ~/.claude and ~/.claude.json aren't touched.
#[test]
fn setup_installs_into_the_claude_config_dir() {
    with_home(|home| {
        let config_dir = home.join("profiles").join("work");
        fs::create_dir_all(&config_dir).unwrap();
        in_config_dir(Some(&config_dir), setup);

        let settings = read_json(&config_dir.join("settings.json"));
        for (event, script) in [
            ("PreToolUse", "graphmind-search.sh"),
            ("SessionStart", "graphmind-session.sh"),
            ("UserPromptSubmit", "graphmind-prompt.sh"),
        ] {
            let path = config_dir.join("hooks").join(script);
            assert!(path.exists(), "{} missing", path.display());
            let command = &settings["hooks"][event][0]["hooks"][0]["command"];
            assert_eq!(command.as_str(), Some(&*path.to_string_lossy()), "{event}: {settings}");
        }
        assert!(config_dir.join("skills/graphmind/SKILL.md").exists());
        assert!(config_dir.join("skills/gm-fn/SKILL.md").exists());
        assert!(fs::read_to_string(config_dir.join("CLAUDE.md")).unwrap().contains("<!-- GM:START -->"));
        let claude_json = read_json(&config_dir.join(".claude.json"));
        assert!(claude_json["mcpServers"]["graphmind"]["command"].is_string(), "{claude_json}");

        assert!(!home.join(".claude").exists(), "~/.claude touched");
        assert!(!home.join(".claude.json").exists(), "~/.claude.json touched");
    });
}

/// Each folder is marked set up on its own: setting up one never marks another complete, so
/// auto-setup still runs for it. ~/.claude keeps the `setup_version` field older versions read,
/// also when CLAUDE_CONFIG_DIR names it.
#[test]
fn setup_versions_are_per_claude_folder() {
    with_home(|home| {
        let work = home.join("profiles").join("work");
        let personal = home.join("profiles").join("personal");
        fs::create_dir_all(&work).unwrap();
        fs::create_dir_all(&personal).unwrap();

        in_config_dir(Some(&work), setup);
        assert_eq!(setup_version_of(Some(&work)), graphmind_config::SETUP_VERSION);
        assert_eq!(setup_version_of(None), 0, "setting up a CLAUDE_CONFIG_DIR folder marked ~/.claude complete");
        assert_eq!(setup_version_of(Some(&personal)), 0);

        in_config_dir(Some(&home.join(".claude")), setup);
        assert_eq!(setup_version_of(None), graphmind_config::SETUP_VERSION, "~/.claude named by the variable");
        assert_eq!(setup_version_of(Some(&personal)), 0, "setting up ~/.claude marked another folder complete");
        let config = graphmind_config::read_config().unwrap();
        assert_eq!(config.setup_versions.len(), 1, "{:?}", config.setup_versions);

        // A trailing slash names the same folder, so the same record.
        assert_eq!(setup_version_of(Some(&home.join("profiles/work/"))), graphmind_config::SETUP_VERSION);
    });
}

/// A folder that is gone loses its setup record, and its auto-setup stamp, at the next setup of
/// any folder: it is never listed or uninstalled again.
#[test]
fn setup_drops_the_state_of_folders_that_are_gone() {
    with_home(|home| {
        let gone = home.join("profiles").join("gone");
        fs::create_dir_all(&gone).unwrap();
        in_config_dir(Some(&gone), setup);
        fs::write(stamp_of(home, &gone), "0 0 0").unwrap();
        assert_eq!(setup_version_of(Some(&gone)), graphmind_config::SETUP_VERSION);

        fs::remove_dir_all(&gone).unwrap();
        setup();

        let config = graphmind_config::read_config().unwrap();
        assert!(config.setup_versions.is_empty(), "{:?}", config.setup_versions);
        assert!(!stamp_of(home, &gone).exists(), "stamp of a deleted folder left in ~/.graphmind");
    });
}

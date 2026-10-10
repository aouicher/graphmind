/// E2E tests for `graphmind setup` individual install functions.
///
/// Each test creates an ephemeral HOME directory via `tempfile::tempdir()`,
/// overrides the `HOME` env var, calls the function under test, then asserts
/// file state.  Tests are serialized with a Mutex because `HOME` is a process-
/// wide env var.
///
/// Run with:
///   cargo test -p graphmind-cli --test setup_e2e -- --test-threads=1
use common::{backups_of, claude_code_step, fake_claude, with_home, FAKE_CLAUDE_OK};
use graphmind_cli::commands::claude_hook::install_hook;
use graphmind_cli::commands::setup::{
    ensure_project_mcp_configs, home_dir, install_mcp_client,
    migrate_project_mcp_entries, repair_claude_settings, McpClient, Outcome,
};
use graphmind_config::claude_home::ClaudeHome;
use serde_json::{json, Value};
use std::fs;

mod common;

/// `graphmind setup` run as a command in `home`, as zsh. Returns its stdout.
fn run_setup(home: &std::path::Path) -> String {
    let out = common::run_setup_as(std::path::Path::new(env!("CARGO_BIN_EXE_graphmind")), home, "/bin/zsh");
    let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
    assert!(out.status.success(), "setup failed\nstdout: {stdout}\nstderr: {}", String::from_utf8_lossy(&out.stderr));
    stdout
}

// ---------------------------------------------------------------------------
// 2. Claude Desktop MCP — written
// ---------------------------------------------------------------------------

fn claude_desktop_dir(home: &std::path::Path) -> std::path::PathBuf {
    if cfg!(target_os = "macos") {
        home.join("Library/Application Support/Claude")
    } else {
        home.join(".config/claude")
    }
}

#[test]
fn setup_claude_desktop_mcp_written() {
    with_home(|home| {
        let config_dir = claude_desktop_dir(home);
        fs::create_dir_all(&config_dir).unwrap();

        assert_eq!(install_mcp_client(&McpClient::ClaudeDesktop), Outcome::Written);

        let config_file = config_dir.join("claude_desktop_config.json");
        assert!(config_file.exists(), "config file should be created");
        let content = fs::read_to_string(&config_file).unwrap();
        let json: Value = serde_json::from_str(&content).expect("valid JSON");

        assert!(
            json["mcpServers"]["graphmind"].is_object(),
            "mcpServers.graphmind should be an object"
        );

        // Issue #105: no env block at all — `command` is absolute, and setting
        // env.PATH would replace the inherited environment (breaking nvm/asdf).
        assert!(
            json["mcpServers"]["graphmind"]["env"].is_null(),
            "entry must carry no env block, got: {}",
            json["mcpServers"]["graphmind"]
        );
        assert!(
            json["mcpServers"]["graphmind"]["command"]
                .as_str()
                .unwrap_or("")
                .starts_with('/'),
            "command must be an absolute path"
        );
    });
}

// ---------------------------------------------------------------------------
// 3. Claude Desktop MCP — idempotent, no backup on a no-op run, stale entry rewritten
// ---------------------------------------------------------------------------

#[test]
fn setup_claude_desktop_mcp_idempotent() {
    with_home(|home| {
        let config_dir = claude_desktop_dir(home);
        fs::create_dir_all(&config_dir).unwrap();
        let config_file = config_dir.join("claude_desktop_config.json");

        assert_eq!(install_mcp_client(&McpClient::ClaudeDesktop), Outcome::Written);
        let first = fs::read_to_string(&config_file).unwrap();
        assert_eq!(install_mcp_client(&McpClient::ClaudeDesktop), Outcome::Unchanged);

        assert_eq!(fs::read_to_string(&config_file).unwrap(), first);
        assert_eq!(first.matches("\"graphmind\"").count(), 1);
        assert_eq!(backups_of(&config_file), 0, "no backup when nothing changes");
    });
}

#[test]
fn setup_claude_desktop_mcp_rewrites_stale_entry() {
    with_home(|home| {
        let config_dir = claude_desktop_dir(home);
        fs::create_dir_all(&config_dir).unwrap();
        let config_file = config_dir.join("claude_desktop_config.json");
        fs::write(
            &config_file,
            json!({ "mcpServers": { "graphmind": { "command": "/moved/graphmind", "args": ["mcp"] } }, "theme": "dark" })
                .to_string(),
        )
        .unwrap();

        assert_eq!(install_mcp_client(&McpClient::ClaudeDesktop), Outcome::Written);

        let json: Value = serde_json::from_str(&fs::read_to_string(&config_file).unwrap()).unwrap();
        assert_ne!(json["mcpServers"]["graphmind"]["command"].as_str(), Some("/moved/graphmind"));
        assert_eq!(json["theme"], "dark");
        assert_eq!(backups_of(&config_file), 1);
    });
}

// ---------------------------------------------------------------------------
// 4. Claude Code MCP — user scope in ~/.claude.json
// ---------------------------------------------------------------------------

#[test]
fn setup_claude_code_mcp_written() {
    with_home(|home| {
        let claude_dir = home.join(".claude");
        fs::create_dir_all(&claude_dir).unwrap();

        assert_eq!(claude_code_step(), Outcome::Written);

        // User scope, where `claude mcp add -s user` puts servers.
        let content = fs::read_to_string(home.join(".claude.json")).unwrap();
        let json: Value = serde_json::from_str(&content).expect("valid JSON");
        let entry = &json["mcpServers"]["graphmind"];
        assert_eq!(entry["type"], "stdio");
        // Issue #105: the MCP entry must not pin PATH.
        assert!(entry["env"].is_null(), "MCP entry must carry no env block, got: {entry}");

        // Nothing to change in settings.json: it isn't created, so it gets no env.PATH.
        assert!(!claude_dir.join("settings.json").exists());
    });
}

// ---------------------------------------------------------------------------
// 4b. Issue #105 — a legacy env.PATH is repaired in place, the old entry moves to user scope
// ---------------------------------------------------------------------------

#[test]
fn setup_repairs_legacy_env_path() {
    with_home(|home| {
        let claude_dir = home.join(".claude");
        fs::create_dir_all(&claude_dir).unwrap();
        let bin_dir = home.join(".graphmind").join("bin");
        let bin_str = bin_dir.to_str().unwrap().to_string();

        // Simulate a pre-#105 install where the user later added their own nvm
        // path by hand — that edit must survive the repair.
        let nvm = "/Users/test/.nvm/versions/node/v20.11.0/bin";
        let settings = claude_dir.join("settings.json");
        fs::write(
            &settings,
            serde_json::to_string_pretty(&json!({
                "env": { "PATH": format!("{bin_str}:{nvm}:/usr/local/bin:/usr/bin:/bin") },
                "mcpServers": {
                    "graphmind": {
                        "command": "/stale/path/graphmind",
                        "args": ["mcp"],
                        "env": { "PATH": format!("{bin_str}:/usr/local/bin:/usr/bin:/bin") }
                    }
                }
            }))
            .unwrap(),
        )
        .unwrap();

        claude_code_step();

        let json: Value =
            serde_json::from_str(&fs::read_to_string(&settings).unwrap()).expect("valid JSON");

        // Our segment is gone, the user's nvm entry is preserved and still ordered.
        let global_path = json["env"]["PATH"].as_str().unwrap_or("");
        assert!(
            !global_path.contains(&bin_str),
            "graphmind segment should be stripped, got: {global_path}"
        );
        assert!(
            global_path.contains(nvm),
            "user-added nvm path must be preserved, got: {global_path}"
        );

        // The stale settings.json entry is gone; the user-scope one has no env and the real binary.
        assert!(json["mcpServers"].is_null(), "emptied mcpServers removed, got: {json}");
        let user: Value = serde_json::from_str(&fs::read_to_string(home.join(".claude.json")).unwrap()).unwrap();
        let entry = &user["mcpServers"]["graphmind"];
        assert!(entry["env"].is_null(), "no env block, got: {entry}");
        assert_ne!(entry["command"].as_str(), Some("/stale/path/graphmind"));
    });
}

// ---------------------------------------------------------------------------
// 4c. Issue #105 — repair drops env.PATH entirely when only the old default remains
// ---------------------------------------------------------------------------

#[test]
fn setup_removes_env_path_when_only_legacy_default_remains() {
    with_home(|home| {
        let claude_dir = home.join(".claude");
        fs::create_dir_all(&claude_dir).unwrap();
        let bin_str = home.join(".graphmind").join("bin").to_str().unwrap().to_string();

        let settings = claude_dir.join("settings.json");
        fs::write(
            &settings,
            serde_json::to_string_pretty(&json!({
                "env": { "PATH": format!("{bin_str}:/usr/local/bin:/usr/bin:/bin") }
            }))
            .unwrap(),
        )
        .unwrap();

        claude_code_step();

        let json: Value =
            serde_json::from_str(&fs::read_to_string(&settings).unwrap()).expect("valid JSON");

        // Nothing the user curated remained, so the key is dropped and Claude Code
        // falls back to the inherited shell PATH — what nvm users need.
        assert!(
            json["env"]["PATH"].is_null(),
            "env.PATH should be removed entirely, got: {}",
            json["env"]
        );
    });
}

// ---------------------------------------------------------------------------
// 5. Claude Code MCP — idempotent
// ---------------------------------------------------------------------------

#[test]
fn setup_claude_code_mcp_idempotent() {
    with_home(|home| {
        fs::create_dir_all(home.join(".claude")).unwrap();

        assert_eq!(claude_code_step(), Outcome::Written);
        let first = fs::read_to_string(home.join(".claude.json")).unwrap();
        assert_eq!(claude_code_step(), Outcome::Unchanged);
        let second = fs::read_to_string(home.join(".claude.json")).unwrap();

        assert_eq!(first, second, "a second run must not rewrite ~/.claude.json");
        assert_eq!(backups_of(&home.join(".claude.json")), 0);
        let json: Value = serde_json::from_str(&second).expect("valid JSON");
        assert!(json["mcpServers"]["graphmind"].is_object());
    });
}

// ---------------------------------------------------------------------------
// 5b. Claude Code MCP — settings repair and registration are independent
// ---------------------------------------------------------------------------

#[test]
fn setup_registers_user_scope_even_if_settings_json_is_broken() {
    with_home(|home| {
        let settings = home.join(".claude").join("settings.json");
        fs::create_dir_all(settings.parent().unwrap()).unwrap();
        let broken = "{\n  \"mcpServers\": {\"graphmind\": {\"command\": \"/old\"}},\n}\n"; // trailing comma
        fs::write(&settings, broken).unwrap();
        fs::write(home.join(".claude.json"), r#"{"numStartups": 7}"#).unwrap();

        assert_eq!(claude_code_step(), Outcome::Written);

        assert_eq!(fs::read_to_string(&settings).unwrap(), broken, "unparseable settings.json untouched");
        let user: Value = serde_json::from_str(&fs::read_to_string(home.join(".claude.json")).unwrap()).unwrap();
        assert!(user["mcpServers"]["graphmind"]["command"].is_string(), "{user}");
        assert_eq!(user["numStartups"], 7, "rest of ~/.claude.json kept");
    });
}

#[test]
fn setup_keeps_legacy_entries_when_user_scope_registration_fails() {
    with_home(|home| {
        let settings = home.join(".claude").join("settings.json");
        fs::create_dir_all(settings.parent().unwrap()).unwrap();
        let legacy = json!({ "mcpServers": { "graphmind": { "command": "/old/graphmind", "args": ["mcp"] } } });
        fs::write(&settings, legacy.to_string()).unwrap();
        let claude_json = home.join(".claude.json");
        let half_written = r#"{"projects": {"/p": {"mcpServers": {"graphmind": {"command": "/old/graphmind"}}}"#;
        fs::write(&claude_json, half_written).unwrap();

        let outcome = claude_code_step();
        migrate_project_mcp_entries(outcome.is_in_place());

        assert!(matches!(outcome, Outcome::LeftAlone(_)), "{outcome:?}");
        assert_eq!(fs::read_to_string(&claude_json).unwrap(), half_written, "~/.claude.json untouched");
        let json: Value = serde_json::from_str(&fs::read_to_string(&settings).unwrap()).unwrap();
        assert_eq!(json, legacy, "the only working entry stays until its replacement is in place");
    });
}

// ---------------------------------------------------------------------------
// 5c. Claude Code MCP — stale project-scope entries removed once user scope is in place
// ---------------------------------------------------------------------------

#[test]
fn setup_migrates_project_scope_entries_to_user_scope() {
    with_home(|home| {
        let claude_json = home.join(".claude.json");
        fs::write(
            &claude_json,
            serde_json::to_string_pretty(&json!({
                "numStartups": 3,
                "projects": {
                    "/repo/a": {
                        "allowedTools": [],
                        "mcpServers": {
                            "graphmind": { "type": "stdio", "command": "/old/graphmind", "args": ["mcp"] },
                            "other": { "command": "other" }
                        }
                    },
                    "/repo/b": { "mcpServers": { "graphmind": { "command": "/old/graphmind", "env": { "PATH": "/usr/bin" } } } },
                    "/repo/c": { "mcpServers": {} }
                }
            }))
            .unwrap(),
        )
        .unwrap();

        // Not yet confirmed: nothing is removed.
        migrate_project_mcp_entries(false);
        let json: Value = serde_json::from_str(&fs::read_to_string(&claude_json).unwrap()).unwrap();
        assert!(json["projects"]["/repo/a"]["mcpServers"]["graphmind"].is_object());

        let outcome = claude_code_step();
        assert_eq!(outcome, Outcome::Written);
        migrate_project_mcp_entries(outcome.is_in_place());

        let json: Value = serde_json::from_str(&fs::read_to_string(&claude_json).unwrap()).unwrap();
        assert!(json["projects"]["/repo/a"]["mcpServers"]["graphmind"].is_null(), "{json}");
        assert!(json["projects"]["/repo/b"]["mcpServers"]["graphmind"].is_null(), "{json}");
        assert_eq!(json["projects"]["/repo/a"]["mcpServers"]["other"]["command"], "other");
        assert!(json["projects"]["/repo/a"]["allowedTools"].is_array());
        assert_eq!(json["numStartups"], 3);
        assert!(json["mcpServers"]["graphmind"]["command"].is_string(), "user-scope entry in place");
    });
}

// ---------------------------------------------------------------------------
// 5d. Claude Code MCP — the `claude` CLI does the write when it is on PATH
// ---------------------------------------------------------------------------

#[cfg(unix)]
#[test]
fn setup_registers_through_claude_cli_when_available() {
    with_home(|home| {
        fake_claude(home, FAKE_CLAUDE_OK);
        // A stale entry: the CLI refuses to add over it, so it is removed first.
        fs::write(home.join(".claude.json"), r#"{"mcpServers": {"graphmind": {"command": "/old/graphmind"}}}"#).unwrap();

        assert_eq!(install_mcp_client(&McpClient::claude_code()), Outcome::Written);

        let log = fs::read_to_string(home.join("claude-calls.log")).unwrap();
        let bin = home.join(".graphmind/bin/graphmind");
        assert_eq!(
            log,
            format!("mcp remove -s user graphmind\nmcp add -s user graphmind -- {} mcp\n", bin.display())
        );
        assert_eq!(backups_of(&home.join(".claude.json")), 0, "the CLI wrote the file, not graphmind");

        // What the CLI wrote (with an empty `env`) counts as current: no second call.
        assert_eq!(install_mcp_client(&McpClient::claude_code()), Outcome::Unchanged);
        assert_eq!(fs::read_to_string(home.join("claude-calls.log")).unwrap(), log);
    });
}

#[cfg(unix)]
#[test]
fn setup_falls_back_to_direct_write_when_claude_cli_fails() {
    with_home(|home| {
        fake_claude(home, "echo 'config locked' >&2\nexit 1");
        fs::write(home.join(".claude.json"), r#"{"numStartups": 7}"#).unwrap();

        assert_eq!(install_mcp_client(&McpClient::claude_code()), Outcome::Written);

        assert!(fs::read_to_string(home.join("claude-calls.log")).unwrap().starts_with("mcp add"));
        let user: Value = serde_json::from_str(&fs::read_to_string(home.join(".claude.json")).unwrap()).unwrap();
        assert!(user["mcpServers"]["graphmind"]["command"].is_string(), "{user}");
        assert_eq!(user["numStartups"], 7);
    });
}

// ---------------------------------------------------------------------------
// 6. OpenCode MCP — written
// ---------------------------------------------------------------------------

#[test]
fn setup_opencode_mcp_written() {
    with_home(|home| {
        fs::create_dir_all(home.join(".config/opencode")).unwrap(); // OpenCode installed
        assert_eq!(install_mcp_client(&McpClient::OpenCode), Outcome::Written);

        let config_path = home
            .join(".config")
            .join("opencode")
            .join("opencode.jsonc");
        assert!(config_path.exists(), "opencode.jsonc should be created");

        let content = fs::read_to_string(&config_path).unwrap();
        let json: Value = serde_json::from_str(&content).expect("valid JSON (no comments written)");

        assert!(
            json["mcp"]["graphmind"].is_object(),
            "mcp.graphmind should exist"
        );
        assert_eq!(
            json["mcp"]["graphmind"]["type"].as_str(),
            Some("local"),
            "type should be \"local\""
        );
        assert!(
            json["mcp"]["graphmind"]["command"].is_array(),
            "command should be an array"
        );

        // Issue #105: no environment block — the command array is absolute.
        assert!(
            json["mcp"]["graphmind"]["environment"].is_null(),
            "entry must carry no environment block, got: {}",
            json["mcp"]["graphmind"]
        );
    });
}

#[cfg(unix)]
#[test]
fn setup_opencode_detected_on_path_before_first_run() {
    with_home(|home| {
        use std::os::unix::fs::PermissionsExt;
        let bin = home.join("bin");
        fs::create_dir_all(&bin).unwrap();
        fs::write(bin.join("opencode"), "#!/bin/sh\n").unwrap();
        fs::set_permissions(bin.join("opencode"), fs::Permissions::from_mode(0o755)).unwrap();

        assert_eq!(install_mcp_client(&McpClient::OpenCode), Outcome::Written);
        assert!(home.join(".config/opencode/opencode.jsonc").exists());
    });
}

// ---------------------------------------------------------------------------
// 7. OpenCode MCP — idempotent
// ---------------------------------------------------------------------------

#[test]
fn setup_opencode_mcp_idempotent() {
    with_home(|home| {
        fs::create_dir_all(home.join(".config/opencode")).unwrap(); // OpenCode installed
        install_mcp_client(&McpClient::OpenCode);
        assert_eq!(install_mcp_client(&McpClient::OpenCode), Outcome::Unchanged);

        let config_path = home_dir()
            .join(".config")
            .join("opencode")
            .join("opencode.jsonc");
        let content = fs::read_to_string(&config_path).unwrap();
        let json: Value = serde_json::from_str(&content).expect("valid JSON");
        let mcp = json["mcp"].as_object().expect("mcp object");
        let count = mcp.keys().filter(|k| *k == "graphmind").count();
        assert_eq!(count, 1, "graphmind should appear exactly once in mcp");
    });
}

// ---------------------------------------------------------------------------
// 8. OpenCode MCP — existing JSONC: entry added unless a rewrite would drop comments
// ---------------------------------------------------------------------------

#[test]
fn setup_opencode_mcp_adds_entry_to_existing_file() {
    with_home(|home| {
        let config_dir = home.join(".config").join("opencode");
        fs::create_dir_all(&config_dir).unwrap();
        let config_path = config_dir.join("opencode.jsonc");

        // No comments; the trailing newline must not count as one.
        fs::write(&config_path, "{\"foo\": \"bar\"}\n").unwrap();

        assert_eq!(install_mcp_client(&McpClient::OpenCode), Outcome::Written);

        let json: Value = serde_json::from_str(&fs::read_to_string(&config_path).unwrap()).unwrap();
        assert_eq!(json["foo"].as_str(), Some("bar"), "existing 'foo' key should be preserved");
        assert!(json["mcp"]["graphmind"].is_object(), "graphmind entry should be added");
    });
}

#[test]
fn setup_opencode_mcp_keeps_jsonc_comments() {
    with_home(|home| {
        let config_dir = home.join(".config").join("opencode");
        fs::create_dir_all(&config_dir).unwrap();
        let config_path = config_dir.join("opencode.jsonc");

        // A JSONC file with a comment: rewriting it as JSON would delete the comment.
        let existing = "// existing config\n{\"foo\": \"bar\"}\n";
        fs::write(&config_path, existing).unwrap();

        assert!(matches!(install_mcp_client(&McpClient::OpenCode), Outcome::LeftAlone(_)));

        assert_eq!(fs::read_to_string(&config_path).unwrap(), existing, "commented JSONC left as is");
        assert_eq!(backups_of(&config_path), 0);
    });
}

#[test]
fn setup_skips_editors_that_are_not_installed() {
    with_home(|home| {
        assert_eq!(install_mcp_client(&McpClient::Cursor), Outcome::NotInstalled);
        assert_eq!(install_mcp_client(&McpClient::OpenCode), Outcome::NotInstalled);
        assert_eq!(install_mcp_client(&McpClient::ClaudeDesktop), Outcome::NotInstalled);
        assert!(!home.join(".cursor").exists(), "no ~/.cursor created");
        assert!(!home.join(".config/opencode").exists(), "no ~/.config/opencode created");
        assert!(!claude_desktop_dir(home).exists(), "no Claude Desktop folder created");
    });
}

#[test]
fn setup_never_replaces_unparseable_json() {
    with_home(|home| {
        let settings = home.join(".claude").join("settings.json");
        fs::create_dir_all(settings.parent().unwrap()).unwrap();
        let broken = "{\n  \"hooks\": {},\n}\n"; // trailing comma
        fs::write(&settings, broken).unwrap();
        repair_claude_settings(true);
        assert_eq!(fs::read_to_string(&settings).unwrap(), broken);

        let claude_json = home.join(".claude.json");
        fs::write(&claude_json, "{\"projects\": {").unwrap(); // mid-write
        assert!(matches!(install_mcp_client(&McpClient::claude_code()), Outcome::LeftAlone(_)));
        migrate_project_mcp_entries(true);
        assert_eq!(fs::read_to_string(&claude_json).unwrap(), "{\"projects\": {");

        fs::create_dir_all(home.join(".cursor")).unwrap();
        fs::write(home.join(".cursor/mcp.json"), "{,}").unwrap();
        assert!(matches!(install_mcp_client(&McpClient::Cursor), Outcome::LeftAlone(_)));
        assert_eq!(fs::read_to_string(home.join(".cursor/mcp.json")).unwrap(), "{,}");
    });
}

// ---------------------------------------------------------------------------
// 9. Cursor global MCP — written
// ---------------------------------------------------------------------------

#[test]
fn setup_cursor_global_mcp_written() {
    with_home(|home| {
        fs::create_dir_all(home.join(".cursor")).unwrap(); // Cursor installed
        assert_eq!(install_mcp_client(&McpClient::Cursor), Outcome::Written);

        let config_path = home.join(".cursor").join("mcp.json");
        assert!(config_path.exists(), "~/.cursor/mcp.json should be created");
        let content = fs::read_to_string(&config_path).unwrap();
        let json: Value = serde_json::from_str(&content).expect("valid JSON");

        assert!(
            json["mcpServers"]["graphmind"].is_object(),
            "mcpServers.graphmind should exist"
        );
        // Issue #105: no env block — `command` is absolute.
        assert!(
            json["mcpServers"]["graphmind"]["env"].is_null(),
            "entry must carry no env block, got: {}",
            json["mcpServers"]["graphmind"]
        );
    });
}

#[test]
fn setup_cursor_global_mcp_idempotent() {
    with_home(|home| {
        fs::create_dir_all(home.join(".cursor")).unwrap(); // Cursor installed
        install_mcp_client(&McpClient::Cursor);
        assert_eq!(install_mcp_client(&McpClient::Cursor), Outcome::Unchanged);

        let config_path = home_dir().join(".cursor").join("mcp.json");
        let content = fs::read_to_string(&config_path).unwrap();
        let json: Value = serde_json::from_str(&content).expect("valid JSON");
        let count = json["mcpServers"]
            .as_object()
            .unwrap()
            .keys()
            .filter(|k| *k == "graphmind")
            .count();
        assert_eq!(count, 1, "graphmind should appear exactly once");
    });
}

// ---------------------------------------------------------------------------
// 10. ensure_project_mcp_configs — VS Code only, and only in projects that use it
// ---------------------------------------------------------------------------

#[test]
fn ensure_project_mcp_configs_written() {
    with_home(|home| {
        let project_dir = tempfile::tempdir().unwrap();
        fs::create_dir_all(project_dir.path().join(".vscode")).unwrap(); // a VS Code project
        let project_path = project_dir.path().to_str().unwrap();

        ensure_project_mcp_configs(project_path);

        // Claude Code: nothing per project, the user-scope entry covers it.
        assert!(!home.join(".claude.json").exists(), "no project-scope entry in ~/.claude.json");

        let abs_path = std::path::Path::new(project_path)
            .canonicalize()
            .unwrap_or_else(|_| std::path::PathBuf::from(project_path));
        let vscode_mcp = abs_path.join(".vscode").join("mcp.json");
        assert!(vscode_mcp.exists(), ".vscode/mcp.json should be created");
        let content = fs::read_to_string(&vscode_mcp).unwrap();
        let json: Value = serde_json::from_str(&content).expect("valid JSON");
        assert!(
            json["servers"]["graphmind"].is_object(),
            "servers.graphmind should exist in .vscode/mcp.json"
        );
        assert_eq!(
            json["servers"]["graphmind"]["type"].as_str(),
            Some("stdio"),
        );
        // Issue #105: no env block — `command` is absolute.
        assert!(json["servers"]["graphmind"]["env"].is_null());
    });
}

#[test]
fn ensure_project_mcp_configs_skips_projects_without_vscode() {
    with_home(|_home| {
        let project_dir = tempfile::tempdir().unwrap();

        ensure_project_mcp_configs(project_dir.path().to_str().unwrap());

        assert!(!project_dir.path().join(".vscode").exists(), "no .vscode/ created");
    });
}

// ---------------------------------------------------------------------------
// 11. ensure_project_mcp_configs — idempotent
// ---------------------------------------------------------------------------

#[test]
fn ensure_project_mcp_configs_idempotent() {
    with_home(|_home| {
        let project_dir = tempfile::tempdir().unwrap();
        fs::create_dir_all(project_dir.path().join(".vscode")).unwrap();
        let project_path = project_dir.path().to_str().unwrap();

        ensure_project_mcp_configs(project_path);
        ensure_project_mcp_configs(project_path);

        let abs_path = std::path::Path::new(project_path)
            .canonicalize()
            .unwrap_or_else(|_| std::path::PathBuf::from(project_path));
        let vscode_mcp = abs_path.join(".vscode").join("mcp.json");
        let content = fs::read_to_string(&vscode_mcp).unwrap();
        let json: Value = serde_json::from_str(&content).expect("valid JSON");
        let count = json["servers"].as_object().unwrap().keys().filter(|k| *k == "graphmind").count();
        assert_eq!(count, 1, "graphmind should appear exactly once in .vscode/mcp.json");
        assert_eq!(backups_of(&vscode_mcp), 0, "no backup when nothing changes");
    });
}

// ---------------------------------------------------------------------------
// 14. Claude Code hooks — settings.json that can't be used is left alone
// ---------------------------------------------------------------------------

#[test]
fn install_hook_leaves_an_unparseable_settings_json_alone() {
    with_home(|home| {
        let settings = home.join(".claude").join("settings.json");
        fs::create_dir_all(settings.parent().unwrap()).unwrap();
        for broken in [r#"{"permissions": {},}"#, "[1, 2]"] {
            fs::write(&settings, broken).unwrap();
            assert!(install_hook(&ClaudeHome::current()).is_err(), "{broken}");
            assert_eq!(fs::read_to_string(&settings).unwrap(), broken);
            assert_eq!(backups_of(&settings), 0);
        }
    });
}

/// A settings.json whose "hooks" (or one of its events) has the wrong type is left alone, and the
/// reason names the file.
#[test]
fn install_hook_names_the_file_when_hooks_has_the_wrong_type() {
    with_home(|home| {
        let settings = home.join(".claude").join("settings.json");
        fs::create_dir_all(settings.parent().unwrap()).unwrap();
        for broken in [r#"{"hooks": []}"#, r#"{"hooks": {"SessionStart": {}}}"#] {
            fs::write(&settings, broken).unwrap();
            let Err(reason) = install_hook(&ClaudeHome::current()) else { panic!("{broken} was not left alone") };
            assert!(reason.contains(&*settings.to_string_lossy()), "{reason}");
            assert!(reason.ends_with("left unchanged"), "{reason}");
            assert_eq!(fs::read_to_string(&settings).unwrap(), broken);
        }
    });
}

#[cfg(unix)]
#[test]
fn install_hook_leaves_an_unreadable_settings_json_alone() {
    with_home(|home| {
        use std::os::unix::fs::PermissionsExt;
        let settings = home.join(".claude").join("settings.json");
        fs::create_dir_all(settings.parent().unwrap()).unwrap();
        fs::write(&settings, r#"{"permissions": {"allow": ["Bash"]}}"#).unwrap();
        fs::set_permissions(&settings, fs::Permissions::from_mode(0o200)).unwrap(); // write-only
        if common::skip_as_root("install_hook_leaves_an_unreadable_settings_json_alone") {
            return;
        }

        let outcome = install_hook(&ClaudeHome::current());

        fs::set_permissions(&settings, fs::Permissions::from_mode(0o644)).unwrap();
        assert!(outcome.is_err(), "{outcome:?}");
        assert_eq!(fs::read_to_string(&settings).unwrap(), r#"{"permissions": {"allow": ["Bash"]}}"#);
    });
}

/// Every 0.3.2 install registers graphmind-stop.sh. While settings.json is left alone it still
/// points at the script, so the script must stay too.
#[test]
fn install_hook_keeps_retired_scripts_while_settings_json_is_left_alone() {
    with_home(|home| {
        let hooks = home.join(".claude").join("hooks");
        fs::create_dir_all(&hooks).unwrap();
        let stop = hooks.join("graphmind-stop.sh");
        fs::write(&stop, "#!/bin/sh\n").unwrap();
        let settings = home.join(".claude").join("settings.json");
        // A Latin-1 byte: not UTF-8, so the file can't be read as JSON.
        let mut content = br#"{"x": "caf"#.to_vec();
        content.push(0xe9);
        content.extend_from_slice(
            format!(r#"", "hooks": {{"Stop": [{{"matcher": "*", "hooks": [{{"type": "command", "command": "{}"}}]}}]}}}}"#, stop.display())
                .as_bytes(),
        );
        fs::write(&settings, &content).unwrap();

        let outcome = install_hook(&ClaudeHome::current());

        assert!(outcome.is_err(), "{outcome:?}");
        assert_eq!(fs::read(&settings).unwrap(), content, "settings.json changed");
        assert!(stop.exists(), "graphmind-stop.sh removed while settings.json still runs it");
    });
}

#[test]
fn install_hook_creates_a_missing_or_empty_settings_json() {
    with_home(|home| {
        let settings = home.join(".claude").join("settings.json");
        for initial in [None, Some("")] {
            if let Some(content) = initial {
                fs::write(&settings, content).unwrap();
            }
            assert_eq!(install_hook(&ClaudeHome::current()), Ok(()), "{initial:?}");
            let json: Value = serde_json::from_str(&fs::read_to_string(&settings).unwrap()).unwrap();
            for event in ["PreToolUse", "SessionStart", "UserPromptSubmit"] {
                assert!(json["hooks"][event].is_array(), "{initial:?}: {json}");
            }
        }
    });
}

// ---------------------------------------------------------------------------
// 15. Setup summary — from what happened, not a fixed text
// ---------------------------------------------------------------------------

#[test]
fn setup_summary_reports_what_happened_to_hooks_and_skills() {
    with_home(|home| {
        let claude_dir = home.join(".claude");
        let settings = claude_dir.join("settings.json");
        fs::create_dir_all(&claude_dir).unwrap();
        fs::write(&settings, r#"{"permissions": {},}"#).unwrap();
        fs::write(claude_dir.join("skills"), "a file where the skills folder goes").unwrap();

        let out = run_setup(home);
        assert!(out.contains("Claude Code hooks not registered"), "{out}");
        assert!(!out.contains("hooks registered ("), "{out}");
        assert!(out.contains("/gm skills not fully installed"), "{out}");
        assert!(out.contains("Setup complete"), "setup runs to the end: {out}");
        assert_eq!(fs::read_to_string(&settings).unwrap(), r#"{"permissions": {},}"#);

        fs::write(&settings, r#"{"permissions": {}}"#).unwrap();
        fs::remove_file(claude_dir.join("skills")).unwrap();
        let out = run_setup(home);
        assert!(out.contains("3 hooks registered (PreToolUse, SessionStart, UserPromptSubmit)"), "{out}");
        assert!(out.contains("/gm skill + 19 sub-skills installed"), "{out}");
    });
}

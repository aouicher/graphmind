/// E2E tests for `graphmind setup` individual install functions.
///
/// Each test creates an ephemeral HOME directory via `tempfile::tempdir()`,
/// overrides the `HOME` env var, calls the function under test, then asserts
/// file state.  Tests are serialized with a Mutex because `HOME` is a process-
/// wide env var.
///
/// Run with:
///   cargo test -p graphmind-cli --test setup_e2e -- --test-threads=1
use graphmind_cli::commands::setup::{
    already_on_path, ensure_project_mcp_configs, home_dir, install_claude_md_block,
    install_mcp_client, install_shell_path_for, migrate_project_mcp_entries, repair_claude_settings,
    setup, McpClient, Outcome,
};
use serde_json::{json, Value};
use std::fs;
use std::sync::Mutex;

// Serialize all tests — HOME is process-wide.
static TEST_LOCK: Mutex<()> = Mutex::new(());

/// Helper: create a fresh temp HOME, point `HOME` at it, run `f`, then drop. PATH holds the system
/// directories plus `<home>/bin` (empty unless a test puts a fake CLI there), so the `claude` and
/// `opencode` installed on the machine running the tests are never used.
fn with_home<F: FnOnce(&std::path::Path)>(f: F) {
    let _lock = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let dir = tempfile::tempdir().unwrap();
    let path = std::env::join_paths([dir.path().join("bin"), "/usr/bin".into(), "/bin".into()]).unwrap();
    unsafe {
        std::env::set_var("HOME", dir.path());
        std::env::set_var("PATH", path);
        // Claude Code's config would move there, out of the temp HOME.
        std::env::remove_var("CLAUDE_CONFIG_DIR");
    }
    f(dir.path());
    // dir dropped here — content cleaned up, HOME still points at it but that's harmless.
}

/// Setup's Claude Code step: register at user scope, then repair settings.json.
fn claude_code_step() -> Outcome {
    let outcome = install_mcp_client(&McpClient::ClaudeCodeUser);
    repair_claude_settings(outcome.is_in_place());
    outcome
}

/// `.bak` files next to `file`.
fn backups_of(file: &std::path::Path) -> usize {
    let name = file.file_name().unwrap().to_string_lossy().to_string();
    let stem = name.rsplit_once('.').map_or(name.as_str(), |(stem, _)| stem).to_string();
    fs::read_dir(file.parent().unwrap())
        .unwrap()
        .flatten()
        .filter(|e| {
            let n = e.file_name().to_string_lossy().to_string();
            n.starts_with(&stem) && n.ends_with(".bak")
        })
        .count()
}

/// A fake `claude` CLI in `<home>/bin`: logs its arguments to `<home>/claude-calls.log`, then runs
/// `script_body`.
#[cfg(unix)]
fn fake_claude(home: &std::path::Path, script_body: &str) {
    use std::os::unix::fs::PermissionsExt;
    let bin = home.join("bin");
    fs::create_dir_all(&bin).unwrap();
    let script = bin.join("claude");
    fs::write(&script, format!("#!/bin/sh\necho \"$*\" >> \"$HOME/claude-calls.log\"\n{script_body}\n")).unwrap();
    fs::set_permissions(&script, fs::Permissions::from_mode(0o755)).unwrap();
}

/// Writes ~/.claude.json the way `claude mcp add -s user` does (with an empty `env`).
#[cfg(unix)]
const FAKE_CLAUDE_OK: &str = r#"case "$2" in
  add) printf '{"mcpServers":{"graphmind":{"type":"stdio","command":"%s","args":["mcp"],"env":{}}}}' "$7" > "$HOME/.claude.json" ;;
  remove) printf '{"mcpServers":{}}' > "$HOME/.claude.json" ;;
esac"#;

// ---------------------------------------------------------------------------
// 1. Shell PATH
// ---------------------------------------------------------------------------

#[test]
fn setup_shell_path_idempotent() {
    with_home(|home| {
        // Not on PATH, zsh: written once, to .zshenv only.
        let bin_dir = home.join("opt").join("bin");
        install_shell_path_for(&bin_dir.join("graphmind"), "/usr/bin:/bin", "/bin/zsh");
        install_shell_path_for(&bin_dir.join("graphmind"), "/usr/bin:/bin", "/bin/zsh");

        let zshenv = home.join(".zshenv");
        let content = fs::read_to_string(&zshenv).expect(".zshenv should be created");
        let entry = format!("\"{}:$PATH\"", bin_dir.display());
        let count = content.matches(&entry).count();
        assert_eq!(count, 1, "export line should appear exactly once, found {count}:\n{content}");
        assert!(!home.join(".zshrc").exists(), ".zshrc must not be touched for zsh");
    });
}

#[test]
fn setup_shell_path_skips_dir_already_on_path() {
    with_home(|home| {
        let bin_dir = home.join(".local").join("bin");
        fs::create_dir_all(&bin_dir).unwrap();
        let path_env = format!("/usr/bin:{}", bin_dir.display());
        install_shell_path_for(&bin_dir.join("graphmind"), &path_env, "/bin/zsh");
        assert!(!home.join(".zshenv").exists(), "nothing to add when the binary is already on PATH");
    });
}

#[test]
fn setup_shell_path_removes_dead_legacy_line() {
    with_home(|home| {
        // Older setups always added ~/.graphmind/bin, even when the binary lived elsewhere.
        let legacy = format!("export PATH=\"{}:$PATH\"", home.join(".graphmind/bin").display());
        fs::write(home.join(".zshrc"), format!("alias ll='ls -la'\n{legacy}\n")).unwrap();
        fs::write(home.join(".bashrc"), format!("{legacy}\n")).unwrap();

        let bin_dir = home.join(".local").join("bin");
        fs::create_dir_all(&bin_dir).unwrap();
        install_shell_path_for(&bin_dir.join("graphmind"), &bin_dir.display().to_string(), "/bin/zsh");

        assert_eq!(fs::read_to_string(home.join(".zshrc")).unwrap(), "alias ll='ls -la'\n");
        assert!(!fs::read_to_string(home.join(".bashrc")).unwrap().contains(".graphmind/bin"));
    });
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

        assert_eq!(install_mcp_client(&McpClient::ClaudeCodeUser), Outcome::Written);

        let log = fs::read_to_string(home.join("claude-calls.log")).unwrap();
        let bin = home.join(".graphmind/bin/graphmind");
        assert_eq!(
            log,
            format!("mcp remove -s user graphmind\nmcp add -s user graphmind -- {} mcp\n", bin.display())
        );
        assert_eq!(backups_of(&home.join(".claude.json")), 0, "the CLI wrote the file, not graphmind");

        // What the CLI wrote (with an empty `env`) counts as current: no second call.
        assert_eq!(install_mcp_client(&McpClient::ClaudeCodeUser), Outcome::Unchanged);
        assert_eq!(fs::read_to_string(home.join("claude-calls.log")).unwrap(), log);
    });
}

#[cfg(unix)]
#[test]
fn setup_falls_back_to_direct_write_when_claude_cli_fails() {
    with_home(|home| {
        fake_claude(home, "echo 'config locked' >&2\nexit 1");
        fs::write(home.join(".claude.json"), r#"{"numStartups": 7}"#).unwrap();

        assert_eq!(install_mcp_client(&McpClient::ClaudeCodeUser), Outcome::Written);

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
        assert!(matches!(install_mcp_client(&McpClient::ClaudeCodeUser), Outcome::LeftAlone(_)));
        migrate_project_mcp_entries(true);
        assert_eq!(fs::read_to_string(&claude_json).unwrap(), "{\"projects\": {");

        fs::create_dir_all(home.join(".cursor")).unwrap();
        fs::write(home.join(".cursor/mcp.json"), "{,}").unwrap();
        assert!(matches!(install_mcp_client(&McpClient::Cursor), Outcome::LeftAlone(_)));
        assert_eq!(fs::read_to_string(home.join(".cursor/mcp.json")).unwrap(), "{,}");
    });
}

// ---------------------------------------------------------------------------
// 9. CLAUDE.md block — written
// ---------------------------------------------------------------------------

#[test]
fn setup_claude_md_block_written() {
    with_home(|home| {
        let claude_dir = home.join(".claude");
        fs::create_dir_all(&claude_dir).unwrap();

        assert_eq!(install_claude_md_block(), Outcome::Written);

        let claude_md = claude_dir.join("CLAUDE.md");
        assert!(claude_md.exists(), "CLAUDE.md should be created");
        let content = fs::read_to_string(&claude_md).unwrap();
        assert!(
            content.contains("<!-- GM:START -->"),
            "CLAUDE.md should contain GM:START marker"
        );
        assert!(
            content.contains("<!-- GM:END -->"),
            "CLAUDE.md should contain GM:END marker"
        );
    });
}

// ---------------------------------------------------------------------------
// 10. CLAUDE.md block — replaces a stock block from an older version
// ---------------------------------------------------------------------------

/// The block graphmind 0.2.77 wrote, before blocks carried a fingerprint.
const STOCK_BLOCK_0_2_77: &str = r#"<!-- GM:START -->
<!-- GM:VERSION:0.2.77 -->

# graphmind — code intelligence
In any graphmind-registered project, use `/gm` (invoke Skill tool with `skill: "graphmind"`) for ALL code exploration before grep, find, or spawning Explore agents.
- Find symbols, trace callers, view dependencies, file outlines, impact analysis
- Works via CLI — no MCP loading needed
- If you need to understand code structure, call `/gm` first. Only fall back to grep for string literals or config values.

<!-- GM:END -->"#;

#[test]
fn setup_claude_md_block_replaces_existing() {
    with_home(|home| {
        let claude_dir = home.join(".claude");
        fs::create_dir_all(&claude_dir).unwrap();
        let claude_md = claude_dir.join("CLAUDE.md");

        let old_content = format!("# My notes\n\n{STOCK_BLOCK_0_2_77}\n\nother stuff\n");
        fs::write(&claude_md, &old_content).unwrap();

        assert_eq!(install_claude_md_block(), Outcome::Written);

        let content = fs::read_to_string(&claude_md).unwrap();
        let count = content.matches("<!-- GM:START -->").count();
        assert_eq!(
            count, 1,
            "<!-- GM:START --> should appear exactly once after replace, found {count}"
        );
        assert!(content.contains("<!-- GM:HASH:"), "current block written:\n{content}");
        assert!(!content.contains("no MCP loading needed"), "old block replaced:\n{content}");
        assert!(content.starts_with("# My notes\n\n") && content.ends_with("\n\nother stuff\n"));
    });
}

#[test]
fn setup_keeps_an_edited_claude_md_block() {
    with_home(|home| {
        let claude_md = home.join(".claude").join("CLAUDE.md");
        fs::create_dir_all(claude_md.parent().unwrap()).unwrap();

        // First install writes the block with its fingerprint; a second run changes nothing.
        install_claude_md_block();
        let installed = fs::read_to_string(&claude_md).unwrap();
        assert!(installed.contains("<!-- GM:HASH:"));
        assert_eq!(install_claude_md_block(), Outcome::Unchanged);
        assert_eq!(fs::read_to_string(&claude_md).unwrap(), installed);

        // The person rewrites the block in their own words: setup must keep it.
        let start = installed.find("<!-- GM:START -->").unwrap();
        let end = installed.find("<!-- GM:END -->").unwrap();
        let mine = format!(
            "{}<!-- GM:START -->\n<!-- GM:VERSION:0.0.1 -->\nUse graphmind only for callers.\n{}",
            &installed[..start],
            &installed[end..]
        );
        fs::write(&claude_md, &mine).unwrap();
        assert!(matches!(install_claude_md_block(), Outcome::LeftAlone(_)));
        assert_eq!(fs::read_to_string(&claude_md).unwrap(), mine);
        assert_eq!(backups_of(&claude_md), 0, "no backup when nothing changes");
    });
}

#[test]
fn setup_keeps_an_edited_block_from_before_fingerprints() {
    with_home(|home| {
        let claude_md = home.join(".claude").join("CLAUDE.md");
        fs::create_dir_all(claude_md.parent().unwrap()).unwrap();

        // Unmarked blocks that start like a stock one but were edited further down.
        for edited in [
            STOCK_BLOCK_0_2_77.replace("call `/gm` first", "use rg first"),
            "<!-- GM:START -->\n<!-- GM:VERSION:0.2.82 -->\n\n## Mandatory: Code exploration via graphmind\n\nOnly for callers and blast radius.\n\n<!-- GM:END -->".to_string(),
        ] {
            let content = format!("# Mine\n\n{edited}\n");
            fs::write(&claude_md, &content).unwrap();
            assert!(matches!(install_claude_md_block(), Outcome::LeftAlone(_)));
            assert_eq!(fs::read_to_string(&claude_md).unwrap(), content);
        }
    });
}

#[cfg(unix)]
#[test]
fn setup_leaves_an_unreadable_claude_md_alone() {
    with_home(|home| {
        use std::os::unix::fs::PermissionsExt;
        let claude_md = home.join(".claude").join("CLAUDE.md");
        fs::create_dir_all(claude_md.parent().unwrap()).unwrap();
        fs::write(&claude_md, "# My own instructions\n").unwrap();
        fs::set_permissions(&claude_md, fs::Permissions::from_mode(0o200)).unwrap(); // write-only
        if fs::read(&claude_md).is_ok() {
            return; // running as root: permissions don't apply
        }

        assert!(matches!(install_claude_md_block(), Outcome::LeftAlone(_)));

        fs::set_permissions(&claude_md, fs::Permissions::from_mode(0o644)).unwrap();
        assert_eq!(fs::read_to_string(&claude_md).unwrap(), "# My own instructions\n");
        assert_eq!(backups_of(&claude_md), 0);
    });
}

// ---------------------------------------------------------------------------
// 11. CLAUDE.md block — backup created
// ---------------------------------------------------------------------------

#[test]
fn setup_claude_md_block_backup_created() {
    with_home(|home| {
        let claude_dir = home.join(".claude");
        fs::create_dir_all(&claude_dir).unwrap();
        let claude_md = claude_dir.join("CLAUDE.md");

        fs::write(&claude_md, "# Existing CLAUDE.md content\n").unwrap();

        install_claude_md_block();

        // A backup file with .bak extension should have been created.
        assert_eq!(backups_of(&claude_md), 1, "a .bak backup file should be created in ~/.claude/");
    });
}

// ---------------------------------------------------------------------------
// 11. Cursor global MCP — written
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
// 12. ensure_project_mcp_configs — VS Code only, and only in projects that use it
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
// 13. ensure_project_mcp_configs — idempotent
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

/// Linuxbrew / symlinked dev builds: the binary runs from a versioned directory that isn't on
/// PATH, but the link to it is. That counts as "on PATH", so setup adds nothing.
#[cfg(unix)]
#[test]
fn symlinked_binary_on_path_counts_as_on_path() {
    let dir = tempfile::tempdir().unwrap();
    let cellar = dir.path().join("Cellar/graphmind/0.3.2/bin");
    let linked = dir.path().join("linuxbrew/bin");
    let other = dir.path().join("other/bin");
    for d in [&cellar, &linked, &other] {
        std::fs::create_dir_all(d).unwrap();
    }
    let exe = cellar.join("graphmind");
    std::fs::write(&exe, "binary").unwrap();
    std::os::unix::fs::symlink(&exe, linked.join("graphmind")).unwrap();
    std::fs::write(other.join("graphmind"), "a different graphmind").unwrap();

    let path_env = format!("/usr/bin:{}:{}", other.display(), linked.display());
    assert_eq!(already_on_path(&exe, &path_env), Some(linked.join("graphmind")));
    // Another graphmind on PATH, or none, doesn't count.
    assert_eq!(already_on_path(&exe, &format!("/usr/bin:{}", other.display())), None);
    assert_eq!(already_on_path(&exe, "/usr/bin:/bin"), None);
}

/// The whole flow for a symlinked install: the link on PATH is enough, no profile is written.
#[cfg(unix)]
#[test]
fn setup_shell_path_writes_nothing_for_a_linked_binary() {
    with_home(|home| {
        let cellar = home.join("Cellar/graphmind/0.3.2/bin");
        let linked = home.join("linuxbrew/bin");
        fs::create_dir_all(&cellar).unwrap();
        fs::create_dir_all(&linked).unwrap();
        let exe = cellar.join("graphmind");
        fs::write(&exe, "binary").unwrap();
        std::os::unix::fs::symlink(&exe, linked.join("graphmind")).unwrap();

        install_shell_path_for(&exe, &format!("/usr/bin:{}", linked.display()), "/bin/zsh");
        assert!(!home.join(".zshenv").exists(), "the versioned Cellar dir must not be added");
    });
}

// ---------------------------------------------------------------------------
// 14. Setup version — stamped only once the Claude Code entry is in place
// ---------------------------------------------------------------------------

#[test]
fn setup_version_waits_for_the_claude_code_entry() {
    with_home(|home| {
        let setup_version = || graphmind_config::read_config().unwrap().setup_version;
        // Claude Code caught mid-write: the user-scope entry can't be added.
        fs::write(home.join(".claude.json"), r#"{"numStartups": 3,"#).unwrap();
        setup();
        assert_eq!(setup_version(), 0, "auto-setup must retry while the entry is missing");

        fs::write(home.join(".claude.json"), r#"{"numStartups": 3}"#).unwrap();
        setup();
        assert_eq!(setup_version(), graphmind_config::SETUP_VERSION);
    });
}

// ---------------------------------------------------------------------------
// 15. CLAUDE_CONFIG_DIR — where Claude Code (and `claude mcp add`) keeps ~/.claude.json then
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

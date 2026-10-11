/// E2E tests for `graphmind setup` individual install functions.
///
/// Each test creates an ephemeral HOME directory via `tempfile::tempdir()`,
/// overrides the `HOME` env var, calls the function under test, then asserts
/// file state.  Tests are serialized with a Mutex because `HOME` is a process-
/// wide env var.
///
/// Run with:
///   cargo test -p graphmind-cli --test setup_e2e -- --test-threads=1
use common::{backups_of, with_home};
use graphmind_cli::commands::setup::{
    already_on_path, ensure_project_mcp_configs, home_dir, install_claude_code_entries,
    install_mcp_client, install_shell_path_for, repair_claude_settings, McpClient, Outcome,
};
use serde_json::{json, Value};
use std::fs;

mod common;

/// Setup's Claude Code step: the per-project entries, then the settings.json repair. Returns
/// whether the entries are in place.
fn claude_code_step() -> bool {
    let complete = install_claude_code_entries().is_complete();
    repair_claude_settings(complete, None);
    complete
}

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
// 4. Issue #105 — a legacy env.PATH is repaired in place, the old settings.json entry removed
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

        // The settings.json entry (which Claude Code doesn't read) is gone, and no user-scope entry
        // replaces it: Claude Code reads graphmind per project.
        assert!(json["mcpServers"].is_null(), "emptied mcpServers removed, got: {json}");
        assert!(!home.join(".claude.json").exists(), "nothing written to ~/.claude.json");
    });
}

// ---------------------------------------------------------------------------
// 4b. Issue #105 — repair drops env.PATH entirely when only the old default remains
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
// 5. Claude Code MCP — the settings.json entry stays while the entries can't be brought up to date
// ---------------------------------------------------------------------------

#[test]
fn setup_keeps_legacy_entries_when_claude_json_is_unparseable() {
    with_home(|home| {
        let settings = home.join(".claude").join("settings.json");
        fs::create_dir_all(settings.parent().unwrap()).unwrap();
        let legacy = json!({ "mcpServers": { "graphmind": { "command": "/old/graphmind", "args": ["mcp"] } } });
        fs::write(&settings, legacy.to_string()).unwrap();
        let claude_json = home.join(".claude.json");
        let half_written = r#"{"projects": {"/p": {"mcpServers": {"graphmind": {"command": "/old/graphmind"}}}"#;
        fs::write(&claude_json, half_written).unwrap();

        assert!(!claude_code_step());

        assert_eq!(fs::read_to_string(&claude_json).unwrap(), half_written, "~/.claude.json untouched");
        let json: Value = serde_json::from_str(&fs::read_to_string(&settings).unwrap()).unwrap();
        assert_eq!(json, legacy, "kept while the Claude Code entries are not in place");
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
        repair_claude_settings(true, None);
        assert_eq!(fs::read_to_string(&settings).unwrap(), broken);

        let claude_json = home.join(".claude.json");
        fs::write(&claude_json, "{\"projects\": {").unwrap(); // mid-write
        assert!(!install_claude_code_entries().is_complete());
        let project = tempfile::tempdir().unwrap();
        ensure_project_mcp_configs(project.path().to_str().unwrap());
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
// 10. ensure_project_mcp_configs — Claude Code, and VS Code only in projects that use it
// ---------------------------------------------------------------------------

#[test]
fn ensure_project_mcp_configs_written() {
    with_home(|home| {
        let project_dir = tempfile::tempdir().unwrap();
        fs::create_dir_all(project_dir.path().join(".vscode")).unwrap(); // a VS Code project
        let project_path = project_dir.path().to_str().unwrap();

        ensure_project_mcp_configs(project_path);

        let abs_path = std::path::Path::new(project_path)
            .canonicalize()
            .unwrap_or_else(|_| std::path::PathBuf::from(project_path));
        // Claude Code: the project's entry in ~/.claude.json, nothing at user scope.
        let claude: Value = serde_json::from_str(&fs::read_to_string(home.join(".claude.json")).unwrap()).unwrap();
        let entry = &claude["projects"][abs_path.to_str().unwrap()]["mcpServers"]["graphmind"];
        assert_eq!(entry["type"], "stdio", "{claude}");
        assert!(entry["env"].is_null());
        assert!(claude["mcpServers"].is_null());

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

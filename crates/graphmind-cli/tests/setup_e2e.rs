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
    ensure_project_mcp_configs, home_dir, install_claude_desktop_mcp, install_claude_md_block,
    install_opencode_mcp, install_shell_path, register_mcp_in_claude_code,
};
use serde_json::Value;
use std::fs;
use std::sync::Mutex;

// Serialize all tests — HOME is process-wide.
static TEST_LOCK: Mutex<()> = Mutex::new(());

/// Helper: create a fresh temp HOME, point `HOME` at it, run `f`, then drop.
fn with_home<F: FnOnce(&std::path::Path)>(f: F) {
    let _lock = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let dir = tempfile::tempdir().unwrap();
    unsafe { std::env::set_var("HOME", dir.path()) };
    f(dir.path());
    // dir dropped here — content cleaned up, HOME still points at it but that's harmless.
}

// ---------------------------------------------------------------------------
// 1. Shell PATH
// ---------------------------------------------------------------------------

#[test]
fn setup_shell_path_idempotent() {
    with_home(|home| {
        // Call twice — should write the export line exactly once.
        install_shell_path();
        install_shell_path();

        let zshenv = home.join(".zshenv");
        assert!(zshenv.exists(), ".zshenv should be created");
        let content = fs::read_to_string(&zshenv).unwrap();
        let bin_dir = home.join(".graphmind").join("bin");
        let export_line = format!("export PATH=\"{}:$PATH\"", bin_dir.display());
        let count = content.matches(&export_line).count();
        assert_eq!(
            count, 1,
            "export line should appear exactly once, found {count}:\n{content}"
        );
    });
}

// ---------------------------------------------------------------------------
// 2. Claude Desktop MCP — written
// ---------------------------------------------------------------------------

#[test]
fn setup_claude_desktop_mcp_written() {
    with_home(|home| {
        // Create the config dir (on Linux the code will pick .config/claude/)
        let config_dir = if cfg!(target_os = "macos") {
            home.join("Library/Application Support/Claude")
        } else {
            home.join(".config/claude")
        };
        fs::create_dir_all(&config_dir).unwrap();

        install_claude_desktop_mcp();

        let config_file = config_dir.join("claude_desktop_config.json");
        assert!(config_file.exists(), "config file should be created");
        let content = fs::read_to_string(&config_file).unwrap();
        let json: Value = serde_json::from_str(&content).expect("valid JSON");

        assert!(
            json["mcpServers"]["graphmind"].is_object(),
            "mcpServers.graphmind should be an object"
        );

        let path_val = json["mcpServers"]["graphmind"]["env"]["PATH"]
            .as_str()
            .unwrap_or("");
        let bin_dir = home.join(".graphmind").join("bin");
        assert!(
            path_val.contains(bin_dir.to_str().unwrap()),
            "env.PATH should contain .graphmind/bin, got: {path_val}"
        );
    });
}

// ---------------------------------------------------------------------------
// 3. Claude Desktop MCP — idempotent
// ---------------------------------------------------------------------------

#[test]
fn setup_claude_desktop_mcp_idempotent() {
    with_home(|home| {
        let config_dir = if cfg!(target_os = "macos") {
            home.join("Library/Application Support/Claude")
        } else {
            home.join(".config/claude")
        };
        fs::create_dir_all(&config_dir).unwrap();

        install_claude_desktop_mcp();
        install_claude_desktop_mcp();

        let config_file = config_dir.join("claude_desktop_config.json");
        let content = fs::read_to_string(&config_file).unwrap();
        let count = content.matches("\"graphmind\"").count();
        assert_eq!(
            count, 1,
            "\"graphmind\" key should appear exactly once, found {count}"
        );
    });
}

// ---------------------------------------------------------------------------
// 4. Claude Code MCP — written
// ---------------------------------------------------------------------------

#[test]
fn setup_claude_code_mcp_written() {
    with_home(|home| {
        let claude_dir = home.join(".claude");
        fs::create_dir_all(&claude_dir).unwrap();

        register_mcp_in_claude_code();

        let settings_path = claude_dir.join("settings.json");
        assert!(settings_path.exists(), "settings.json should be created");
        let content = fs::read_to_string(&settings_path).unwrap();
        let json: Value = serde_json::from_str(&content).expect("valid JSON");

        assert!(
            json["mcpServers"]["graphmind"].is_object(),
            "mcpServers.graphmind should exist"
        );

        let mcp_path = json["mcpServers"]["graphmind"]["env"]["PATH"]
            .as_str()
            .unwrap_or("");
        let bin_dir = home.join(".graphmind").join("bin");
        let bin_str = bin_dir.to_str().unwrap();
        assert!(
            mcp_path.contains(bin_str),
            "mcpServers.graphmind.env.PATH should contain .graphmind/bin, got: {mcp_path}"
        );

        let global_path = json["env"]["PATH"].as_str().unwrap_or("");
        assert!(
            global_path.contains(bin_str),
            "top-level env.PATH should contain .graphmind/bin, got: {global_path}"
        );
    });
}

// ---------------------------------------------------------------------------
// 5. Claude Code MCP — idempotent
// ---------------------------------------------------------------------------

#[test]
fn setup_claude_code_mcp_idempotent() {
    with_home(|home| {
        let claude_dir = home.join(".claude");
        fs::create_dir_all(&claude_dir).unwrap();

        register_mcp_in_claude_code();
        register_mcp_in_claude_code();

        let settings_path = claude_dir.join("settings.json");
        let content = fs::read_to_string(&settings_path).unwrap();
        // Count occurrences of the key name inside mcpServers (not in the command path)
        let json: Value = serde_json::from_str(&content).expect("valid JSON");
        let mcp_servers = json["mcpServers"].as_object().expect("mcpServers object");
        assert!(mcp_servers.contains_key("graphmind"), "graphmind key should be present");
        // The key "graphmind" inside mcpServers must appear exactly once.
        // Re-running should NOT duplicate it (already_correct guard in source).
        let count = mcp_servers.keys().filter(|k| *k == "graphmind").count();
        assert_eq!(count, 1, "graphmind should appear exactly once");
    });
}

// ---------------------------------------------------------------------------
// 6. OpenCode MCP — written
// ---------------------------------------------------------------------------

#[test]
fn setup_opencode_mcp_written() {
    with_home(|home| {
        install_opencode_mcp();

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

        let env_path = json["mcp"]["graphmind"]["environment"]["PATH"]
            .as_str()
            .unwrap_or("");
        let bin_dir = home.join(".graphmind").join("bin");
        assert!(
            env_path.contains(bin_dir.to_str().unwrap()),
            "environment.PATH should contain .graphmind/bin, got: {env_path}"
        );
    });
}

// ---------------------------------------------------------------------------
// 7. OpenCode MCP — idempotent
// ---------------------------------------------------------------------------

#[test]
fn setup_opencode_mcp_idempotent() {
    with_home(|_home| {
        install_opencode_mcp();
        install_opencode_mcp();

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
// 8. OpenCode MCP — preserves existing JSONC content
// ---------------------------------------------------------------------------

#[test]
fn setup_opencode_mcp_parses_existing_jsonc() {
    with_home(|home| {
        let config_dir = home.join(".config").join("opencode");
        fs::create_dir_all(&config_dir).unwrap();
        let config_path = config_dir.join("opencode.jsonc");

        // Write a JSONC file with a comment and an existing key.
        let existing = "// existing config\n{\"foo\": \"bar\"}\n";
        fs::write(&config_path, existing).unwrap();

        install_opencode_mcp();

        let content = fs::read_to_string(&config_path).unwrap();
        let json: Value = serde_json::from_str(&content).expect("valid JSON after write");

        assert_eq!(
            json["foo"].as_str(),
            Some("bar"),
            "existing 'foo' key should be preserved"
        );
        assert!(
            json["mcp"]["graphmind"].is_object(),
            "graphmind entry should be added"
        );
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

        install_claude_md_block();

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
// 10. CLAUDE.md block — replaces existing block
// ---------------------------------------------------------------------------

#[test]
fn setup_claude_md_block_replaces_existing() {
    with_home(|home| {
        let claude_dir = home.join(".claude");
        fs::create_dir_all(&claude_dir).unwrap();
        let claude_md = claude_dir.join("CLAUDE.md");

        // Write an existing (old) GM block.
        let old_content =
            "# My notes\n\n<!-- GM:START -->\nold content here\n<!-- GM:END -->\n\nother stuff\n";
        fs::write(&claude_md, old_content).unwrap();

        install_claude_md_block();

        let content = fs::read_to_string(&claude_md).unwrap();
        let count = content.matches("<!-- GM:START -->").count();
        assert_eq!(
            count, 1,
            "<!-- GM:START --> should appear exactly once after replace, found {count}"
        );
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
        let bak_files: Vec<_> = fs::read_dir(&claude_dir)
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| {
                e.file_name()
                    .to_string_lossy()
                    .ends_with(".bak")
            })
            .collect();
        assert!(
            !bak_files.is_empty(),
            "a .bak backup file should be created in ~/.claude/"
        );
    });
}

// ---------------------------------------------------------------------------
// 12. ensure_project_mcp_configs — written
// ---------------------------------------------------------------------------

#[test]
fn ensure_project_mcp_configs_written() {
    with_home(|home| {
        let project_dir = tempfile::tempdir().unwrap();
        let project_path = project_dir.path().to_str().unwrap();

        ensure_project_mcp_configs(project_path);

        // 1. ~/.claude.json — project-scoped entry
        let claude_json = home.join(".claude.json");
        assert!(claude_json.exists(), "~/.claude.json should be created");
        let content = fs::read_to_string(&claude_json).unwrap();
        let json: Value = serde_json::from_str(&content).expect("valid JSON");
        let abs_path = std::path::Path::new(project_path)
            .canonicalize()
            .unwrap_or_else(|_| std::path::PathBuf::from(project_path));
        let abs_str = abs_path.to_str().unwrap();
        assert!(
            json["projects"][abs_str]["mcpServers"]["graphmind"].is_object(),
            "projects.<path>.mcpServers.graphmind should exist in ~/.claude.json"
        );
        assert_eq!(
            json["projects"][abs_str]["mcpServers"]["graphmind"]["type"].as_str(),
            Some("stdio"),
        );
        let bin_dir = home.join(".graphmind").join("bin");
        let path_val = json["projects"][abs_str]["mcpServers"]["graphmind"]["env"]["PATH"]
            .as_str()
            .unwrap_or("");
        assert!(
            path_val.contains(bin_dir.to_str().unwrap()),
            "env.PATH should contain .graphmind/bin"
        );

        // 2. <project>/.cursor/mcp.json
        let cursor_mcp = abs_path.join(".cursor").join("mcp.json");
        assert!(cursor_mcp.exists(), ".cursor/mcp.json should be created");
        let content = fs::read_to_string(&cursor_mcp).unwrap();
        let json: Value = serde_json::from_str(&content).expect("valid JSON");
        assert!(
            json["mcpServers"]["graphmind"].is_object(),
            "mcpServers.graphmind should exist in .cursor/mcp.json"
        );

        // 3. <project>/.vscode/mcp.json
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
    });
}

// ---------------------------------------------------------------------------
// 13. ensure_project_mcp_configs — idempotent
// ---------------------------------------------------------------------------

#[test]
fn ensure_project_mcp_configs_idempotent() {
    with_home(|_home| {
        let project_dir = tempfile::tempdir().unwrap();
        let project_path = project_dir.path().to_str().unwrap();

        ensure_project_mcp_configs(project_path);
        ensure_project_mcp_configs(project_path);

        let abs_path = std::path::Path::new(project_path)
            .canonicalize()
            .unwrap_or_else(|_| std::path::PathBuf::from(project_path));

        // ~/.claude.json — graphmind key should appear exactly once per project
        let claude_json = home_dir().join(".claude.json");
        let content = fs::read_to_string(&claude_json).unwrap();
        let json: Value = serde_json::from_str(&content).expect("valid JSON");
        let abs_str = abs_path.to_str().unwrap();
        let mcp_servers = json["projects"][abs_str]["mcpServers"]
            .as_object()
            .expect("mcpServers object");
        let count = mcp_servers.keys().filter(|k| *k == "graphmind").count();
        assert_eq!(count, 1, "graphmind should appear exactly once in ~/.claude.json mcpServers");

        // .cursor/mcp.json
        let cursor_mcp = abs_path.join(".cursor").join("mcp.json");
        let content = fs::read_to_string(&cursor_mcp).unwrap();
        let json: Value = serde_json::from_str(&content).expect("valid JSON");
        let count = json["mcpServers"].as_object().unwrap().keys().filter(|k| *k == "graphmind").count();
        assert_eq!(count, 1, "graphmind should appear exactly once in .cursor/mcp.json");

        // .vscode/mcp.json
        let vscode_mcp = abs_path.join(".vscode").join("mcp.json");
        let content = fs::read_to_string(&vscode_mcp).unwrap();
        let json: Value = serde_json::from_str(&content).expect("valid JSON");
        let count = json["servers"].as_object().unwrap().keys().filter(|k| *k == "graphmind").count();
        assert_eq!(count, 1, "graphmind should appear exactly once in .vscode/mcp.json");
    });
}

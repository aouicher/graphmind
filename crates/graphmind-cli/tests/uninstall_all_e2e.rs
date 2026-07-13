/// E2E tests for `graphmind uninstall all` — reverses everything `setup()`/`init()`
/// install functions do, without ever touching unrelated content.
///
/// Each test creates an ephemeral HOME directory via `tempfile::tempdir()`,
/// overrides the `HOME` env var, calls the function(s) under test, then asserts
/// file state.  Tests are serialized with a Mutex because `HOME` is a process-
/// wide env var.
///
/// Run with:
///   cargo test -p graphmind-cli --test uninstall_all_e2e -- --test-threads=1
use graphmind_cli::commands::claude_hook::{install_hook, uninstall_hook};
use graphmind_cli::commands::install_skill::{install_skill, uninstall_skill};
use graphmind_cli::commands::setup::{
    ensure_project_mcp_configs, home_dir, install_claude_desktop_mcp, install_claude_md_block,
    install_cursor_global_mcp, install_opencode_mcp, install_shell_path,
    register_mcp_in_claude_code, uninstall_claude_desktop_mcp, uninstall_claude_md_block,
    uninstall_cursor_global_mcp, uninstall_opencode_mcp, uninstall_project_mcp_configs,
    uninstall_shell_path, unregister_mcp_in_claude_code,
};
use serde_json::{json, Value};
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
// 1. Shell PATH — removes only the graphmind line, unrelated content survives
// ---------------------------------------------------------------------------

#[test]
fn uninstall_shell_path_preserves_unrelated_content() {
    with_home(|home| {
        let zshrc = home.join(".zshrc");
        let unrelated_before = "# my custom aliases\nalias ll='ls -la'\nexport EDITOR=vim\n";
        let unrelated_after = "\n# more stuff after\nexport FOO=bar\n";
        fs::write(&zshrc, unrelated_before).unwrap();

        // Simulate what install_shell_path does, but also append unrelated content after.
        install_shell_path();
        let content = fs::read_to_string(&zshrc).unwrap();
        let content = format!("{}{}", content, unrelated_after);
        fs::write(&zshrc, &content).unwrap();

        uninstall_shell_path();

        let final_content = fs::read_to_string(&zshrc).unwrap();

        // The graphmind export line must be gone.
        let bin_dir = home.join(".graphmind").join("bin");
        let export_line = format!("export PATH=\"{}:$PATH\"", bin_dir.display());
        assert!(
            !final_content.contains(&export_line),
            "graphmind export line should be removed, got:\n{final_content}"
        );

        // Unrelated content must survive verbatim.
        assert!(
            final_content.contains("alias ll='ls -la'"),
            "unrelated alias should survive, got:\n{final_content}"
        );
        assert!(
            final_content.contains("export EDITOR=vim"),
            "unrelated EDITOR export should survive, got:\n{final_content}"
        );
        assert!(
            final_content.contains("export FOO=bar"),
            "unrelated FOO export should survive, got:\n{final_content}"
        );
        assert!(
            final_content.contains("# my custom aliases"),
            "unrelated comment should survive, got:\n{final_content}"
        );
    });
}

#[test]
fn uninstall_shell_path_noop_when_absent() {
    with_home(|home| {
        let zshrc = home.join(".zshrc");
        let content = "# nothing graphmind related here\nexport PATH=\"/usr/local/bin:$PATH\"\n";
        fs::write(&zshrc, content).unwrap();

        uninstall_shell_path();

        let final_content = fs::read_to_string(&zshrc).unwrap();
        assert_eq!(final_content, content, "file without graphmind line should be untouched");
    });
}

// ---------------------------------------------------------------------------
// 2. Claude Code MCP settings.json — removes mcpServers.graphmind + PATH prefix,
//    unrelated mcpServers entry AND unrelated top-level key survive
// ---------------------------------------------------------------------------

#[test]
fn unregister_mcp_in_claude_code_preserves_unrelated() {
    with_home(|home| {
        let claude_dir = home.join(".claude");
        fs::create_dir_all(&claude_dir).unwrap();

        register_mcp_in_claude_code();

        let settings_path = claude_dir.join("settings.json");
        let content = fs::read_to_string(&settings_path).unwrap();
        let mut json: Value = serde_json::from_str(&content).unwrap();

        // Inject an unrelated mcpServers entry and an unrelated top-level key.
        json["mcpServers"]["some-other-server"] = json!({"command": "other", "args": []});
        json["someUnrelatedKey"] = json!("keep-me");
        fs::write(&settings_path, serde_json::to_string_pretty(&json).unwrap()).unwrap();

        unregister_mcp_in_claude_code();

        let content = fs::read_to_string(&settings_path).unwrap();
        let json: Value = serde_json::from_str(&content).unwrap();

        assert!(
            json["mcpServers"]["graphmind"].is_null(),
            "mcpServers.graphmind should be removed, got: {json}"
        );
        assert!(
            json["mcpServers"]["some-other-server"].is_object(),
            "unrelated mcpServers entry should survive, got: {json}"
        );
        assert_eq!(
            json["someUnrelatedKey"].as_str(),
            Some("keep-me"),
            "unrelated top-level key should survive, got: {json}"
        );

        // The env.PATH prefix graphmind added should be stripped.
        let bin_dir = home.join(".graphmind").join("bin").to_string_lossy().to_string();
        let path_val = json["env"]["PATH"].as_str().unwrap_or("");
        assert!(
            !path_val.starts_with(&format!("{bin_dir}:")),
            "env.PATH prefix should be stripped, got: {path_val}"
        );
    });
}

#[test]
fn unregister_mcp_in_claude_code_leaves_unrelated_path_untouched() {
    with_home(|home| {
        let claude_dir = home.join(".claude");
        fs::create_dir_all(&claude_dir).unwrap();

        // A settings.json with a PATH that does NOT have the graphmind prefix.
        let settings_path = claude_dir.join("settings.json");
        let original = json!({
            "env": { "PATH": "/usr/local/bin:/usr/bin:/bin" },
            "mcpServers": { "other": { "command": "x" } }
        });
        fs::write(&settings_path, serde_json::to_string_pretty(&original).unwrap()).unwrap();

        unregister_mcp_in_claude_code();

        let content = fs::read_to_string(&settings_path).unwrap();
        let json: Value = serde_json::from_str(&content).unwrap();
        assert_eq!(
            json["env"]["PATH"].as_str(),
            Some("/usr/local/bin:/usr/bin:/bin"),
            "PATH without the exact graphmind prefix must be left untouched"
        );
        assert!(json["mcpServers"]["other"].is_object());
    });
}

// ---------------------------------------------------------------------------
// 3. Claude Desktop MCP config — removes only graphmind entry
// ---------------------------------------------------------------------------

#[test]
fn uninstall_claude_desktop_mcp_preserves_unrelated() {
    with_home(|home| {
        let config_dir = if cfg!(target_os = "macos") {
            home.join("Library/Application Support/Claude")
        } else {
            home.join(".config/claude")
        };
        fs::create_dir_all(&config_dir).unwrap();

        install_claude_desktop_mcp();

        let config_file = config_dir.join("claude_desktop_config.json");
        let content = fs::read_to_string(&config_file).unwrap();
        let mut json: Value = serde_json::from_str(&content).unwrap();
        json["mcpServers"]["sibling"] = json!({"command": "sibling-cmd"});
        fs::write(&config_file, serde_json::to_string_pretty(&json).unwrap()).unwrap();

        uninstall_claude_desktop_mcp();

        let content = fs::read_to_string(&config_file).unwrap();
        let json: Value = serde_json::from_str(&content).unwrap();
        assert!(json["mcpServers"]["graphmind"].is_null(), "graphmind entry should be removed");
        assert!(
            json["mcpServers"]["sibling"].is_object(),
            "sibling mcpServers entry should survive, got: {json}"
        );
    });
}

// ---------------------------------------------------------------------------
// 4. Cursor global MCP config — removes only graphmind entry
// ---------------------------------------------------------------------------

#[test]
fn uninstall_cursor_global_mcp_preserves_unrelated() {
    with_home(|home| {
        install_cursor_global_mcp();

        let config_path = home.join(".cursor").join("mcp.json");
        let content = fs::read_to_string(&config_path).unwrap();
        let mut json: Value = serde_json::from_str(&content).unwrap();
        json["mcpServers"]["sibling"] = json!({"command": "sibling-cmd"});
        fs::write(&config_path, serde_json::to_string_pretty(&json).unwrap()).unwrap();

        uninstall_cursor_global_mcp();

        let content = fs::read_to_string(&config_path).unwrap();
        let json: Value = serde_json::from_str(&content).unwrap();
        assert!(json["mcpServers"]["graphmind"].is_null(), "graphmind entry should be removed");
        assert!(
            json["mcpServers"]["sibling"].is_object(),
            "sibling mcpServers entry should survive, got: {json}"
        );
    });
}

// ---------------------------------------------------------------------------
// 5. OpenCode MCP config — removes only mcp.graphmind
// ---------------------------------------------------------------------------

#[test]
fn uninstall_opencode_mcp_preserves_unrelated() {
    with_home(|home| {
        install_opencode_mcp();

        let config_path = home.join(".config").join("opencode").join("opencode.jsonc");
        let content = fs::read_to_string(&config_path).unwrap();
        let mut json: Value = serde_json::from_str(&content).unwrap();
        json["mcp"]["sibling"] = json!({"type": "local", "command": ["sibling"]});
        fs::write(&config_path, serde_json::to_string_pretty(&json).unwrap()).unwrap();

        uninstall_opencode_mcp();

        let content = fs::read_to_string(&config_path).unwrap();
        let json: Value = serde_json::from_str(&content).unwrap();
        assert!(json["mcp"]["graphmind"].is_null(), "mcp.graphmind should be removed");
        assert!(
            json["mcp"]["sibling"].is_object(),
            "sibling mcp entry should survive, got: {json}"
        );
    });
}

#[test]
fn uninstall_opencode_mcp_noop_when_absent() {
    with_home(|_home| {
        // No file exists at all — must be a safe no-op.
        uninstall_opencode_mcp();
        let config_path = home_dir().join(".config").join("opencode").join("opencode.jsonc");
        assert!(!config_path.exists(), "should not create a file that didn't exist");
    });
}

// ---------------------------------------------------------------------------
// 6. CLAUDE.md block — removes exactly the GM block, unrelated content survives
// ---------------------------------------------------------------------------

#[test]
fn uninstall_claude_md_block_preserves_surrounding_content() {
    with_home(|home| {
        let claude_dir = home.join(".claude");
        fs::create_dir_all(&claude_dir).unwrap();
        let claude_md = claude_dir.join("CLAUDE.md");

        let content = "# Before content\nsome unrelated notes\n\n<!-- GM:START -->\nblock content\n<!-- GM:END -->\n\n# After content\nmore unrelated notes\n";
        fs::write(&claude_md, content).unwrap();

        uninstall_claude_md_block();

        let final_content = fs::read_to_string(&claude_md).unwrap();
        assert!(
            !final_content.contains("<!-- GM:START -->"),
            "GM:START should be removed, got:\n{final_content}"
        );
        assert!(
            !final_content.contains("<!-- GM:END -->"),
            "GM:END should be removed, got:\n{final_content}"
        );
        assert!(
            !final_content.contains("block content"),
            "block body should be removed, got:\n{final_content}"
        );
        assert!(
            final_content.contains("# Before content"),
            "content before block should survive, got:\n{final_content}"
        );
        assert!(
            final_content.contains("some unrelated notes"),
            "content before block should survive, got:\n{final_content}"
        );
        assert!(
            final_content.contains("# After content"),
            "content after block should survive, got:\n{final_content}"
        );
        assert!(
            final_content.contains("more unrelated notes"),
            "content after block should survive, got:\n{final_content}"
        );
    });
}

#[test]
fn uninstall_claude_md_block_noop_when_absent() {
    with_home(|home| {
        let claude_dir = home.join(".claude");
        fs::create_dir_all(&claude_dir).unwrap();
        let claude_md = claude_dir.join("CLAUDE.md");

        let content = "# My notes\nno graphmind block here\n";
        fs::write(&claude_md, content).unwrap();

        uninstall_claude_md_block();

        let final_content = fs::read_to_string(&claude_md).unwrap();
        assert_eq!(final_content, content, "file without GM block should be untouched");

        // No backup should have been created.
        let bak_files: Vec<_> = fs::read_dir(&claude_dir)
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_name().to_string_lossy().ends_with(".bak"))
            .collect();
        assert!(bak_files.is_empty(), "no backup should be created when block is absent");
    });
}

// ---------------------------------------------------------------------------
// 7. Skill uninstall — removes graphmind + all 19 gm-* dirs, unrelated skill survives
// ---------------------------------------------------------------------------

#[test]
fn uninstall_skill_preserves_unrelated_skill_dir() {
    with_home(|home| {
        let skills_base = home.join(".claude").join("skills");
        fs::create_dir_all(&skills_base).unwrap();

        // An unrelated skill directory that must survive.
        let other_skill = skills_base.join("some-other-skill");
        fs::create_dir_all(&other_skill).unwrap();
        fs::write(other_skill.join("SKILL.md"), "unrelated skill content").unwrap();

        install_skill();

        // Sanity: graphmind + all 19 sub-skills exist.
        assert!(skills_base.join("graphmind").exists());
        let sub_skill_names = [
            "gm-search", "gm-fn", "gm-query", "gm-deps", "gm-outline", "gm-impact",
            "gm-who-calls", "gm-map", "gm-memory", "gm-diff", "gm-cross", "gm-dead-code",
            "gm-cycles", "gm-similar", "gm-export", "gm-listeners", "gm-status", "gm-build",
            "gm-file",
        ];
        assert_eq!(sub_skill_names.len(), 19, "sanity: exactly 19 sub-skill names expected");
        for name in &sub_skill_names {
            assert!(skills_base.join(name).exists(), "{name} should exist before uninstall");
        }

        uninstall_skill();

        assert!(!skills_base.join("graphmind").exists(), "graphmind skill dir should be removed");
        for name in &sub_skill_names {
            assert!(!skills_base.join(name).exists(), "{name} should be removed");
        }

        // Unrelated skill directory must survive untouched.
        assert!(other_skill.exists(), "unrelated skill dir should survive");
        assert_eq!(
            fs::read_to_string(other_skill.join("SKILL.md")).unwrap(),
            "unrelated skill content",
            "unrelated skill content should survive verbatim"
        );
    });
}

// ---------------------------------------------------------------------------
// 8. Per-project MCP configs — removes only graphmind entries
// ---------------------------------------------------------------------------

#[test]
fn uninstall_project_mcp_configs_preserves_unrelated_project_and_server() {
    with_home(|home| {
        let project_dir = tempfile::tempdir().unwrap();
        let project_path = project_dir.path().to_str().unwrap();

        ensure_project_mcp_configs(project_path);

        let abs_path = std::path::Path::new(project_path)
            .canonicalize()
            .unwrap_or_else(|_| std::path::PathBuf::from(project_path));
        let abs_str = abs_path.to_string_lossy().to_string();

        // Inject an unrelated second project entry into ~/.claude.json.
        let claude_json_path = home.join(".claude.json");
        let content = fs::read_to_string(&claude_json_path).unwrap();
        let mut json: Value = serde_json::from_str(&content).unwrap();
        json["projects"]["/some/other/project"] = json!({
            "mcpServers": { "other-tool": { "command": "other" } }
        });
        fs::write(&claude_json_path, serde_json::to_string_pretty(&json).unwrap()).unwrap();

        // Inject an unrelated sibling server into .vscode/mcp.json.
        let vscode_mcp = abs_path.join(".vscode").join("mcp.json");
        let content = fs::read_to_string(&vscode_mcp).unwrap();
        let mut json: Value = serde_json::from_str(&content).unwrap();
        json["servers"]["sibling"] = json!({"command": "sibling-cmd"});
        fs::write(&vscode_mcp, serde_json::to_string_pretty(&json).unwrap()).unwrap();

        uninstall_project_mcp_configs(project_path);

        // ~/.claude.json: graphmind gone for our project, unrelated project entry survives,
        // and the project entry itself must still exist (not deleted).
        let content = fs::read_to_string(&claude_json_path).unwrap();
        let json: Value = serde_json::from_str(&content).unwrap();
        assert!(
            json["projects"][&abs_str]["mcpServers"]["graphmind"].is_null(),
            "graphmind entry should be removed for our project, got: {json}"
        );
        assert!(
            json["projects"][&abs_str].is_object(),
            "the project entry itself must not be deleted, got: {json}"
        );
        assert_eq!(
            json["projects"]["/some/other/project"]["mcpServers"]["other-tool"]["command"].as_str(),
            Some("other"),
            "unrelated project entry should survive untouched, got: {json}"
        );

        // .vscode/mcp.json: graphmind gone, sibling server survives, file not deleted.
        assert!(vscode_mcp.exists(), ".vscode/mcp.json must not be deleted");
        let content = fs::read_to_string(&vscode_mcp).unwrap();
        let json: Value = serde_json::from_str(&content).unwrap();
        assert!(json["servers"]["graphmind"].is_null(), "servers.graphmind should be removed");
        assert!(
            json["servers"]["sibling"].is_object(),
            "sibling server should survive, got: {json}"
        );
    });
}

// ---------------------------------------------------------------------------
// 9. Full round-trip: install then uninstall for several pairs, canary survives
// ---------------------------------------------------------------------------

#[test]
fn round_trip_install_then_uninstall_hooks_and_configs() {
    with_home(|home| {
        // --- Claude Code hooks (install_hook / uninstall_hook) ---
        let claude_dir = home.join(".claude");
        fs::create_dir_all(&claude_dir).unwrap();
        let settings_path = claude_dir.join("settings.json");
        // Canary: a pre-existing settings.json with unrelated content.
        let canary_settings = json!({
            "canaryKey": "canary-value",
            "hooks": { "SomeOtherEvent": [{"matcher": "*", "hooks": [{"type": "command", "command": "echo unrelated"}]}] }
        });
        fs::write(&settings_path, serde_json::to_string_pretty(&canary_settings).unwrap()).unwrap();

        install_hook();
        uninstall_hook();

        let content = fs::read_to_string(&settings_path).unwrap();
        let json: Value = serde_json::from_str(&content).unwrap();
        assert_eq!(json["canaryKey"].as_str(), Some("canary-value"), "canary key should survive round-trip");
        assert!(
            json["hooks"]["SomeOtherEvent"].is_array(),
            "unrelated hook event should survive round-trip, got: {json}"
        );
        // No graphmind hook scripts should remain on disk.
        let hooks_dir = claude_dir.join("hooks");
        if hooks_dir.exists() {
            for entry in fs::read_dir(&hooks_dir).unwrap().filter_map(|e| e.ok()) {
                assert!(
                    !entry.file_name().to_string_lossy().starts_with("graphmind-"),
                    "no graphmind-* hook script should remain: {:?}",
                    entry.file_name()
                );
            }
        }

        // --- Cursor global MCP (install_cursor_global_mcp / uninstall_cursor_global_mcp) ---
        let cursor_path = home.join(".cursor").join("mcp.json");
        fs::create_dir_all(cursor_path.parent().unwrap()).unwrap();
        let canary_cursor = json!({ "mcpServers": { "canary-server": { "command": "canary" } } });
        fs::write(&cursor_path, serde_json::to_string_pretty(&canary_cursor).unwrap()).unwrap();

        install_cursor_global_mcp();
        uninstall_cursor_global_mcp();

        let content = fs::read_to_string(&cursor_path).unwrap();
        let json: Value = serde_json::from_str(&content).unwrap();
        assert!(json["mcpServers"]["graphmind"].is_null(), "graphmind should be gone after round-trip");
        assert_eq!(
            json["mcpServers"]["canary-server"]["command"].as_str(),
            Some("canary"),
            "canary cursor server should survive round-trip"
        );

        // --- OpenCode MCP (install_opencode_mcp / uninstall_opencode_mcp) ---
        let opencode_path = home.join(".config").join("opencode").join("opencode.jsonc");
        fs::create_dir_all(opencode_path.parent().unwrap()).unwrap();
        let canary_opencode = json!({ "mcp": { "canary-server": { "type": "local", "command": ["canary"] } } });
        fs::write(&opencode_path, serde_json::to_string_pretty(&canary_opencode).unwrap()).unwrap();

        install_opencode_mcp();
        uninstall_opencode_mcp();

        let content = fs::read_to_string(&opencode_path).unwrap();
        let json: Value = serde_json::from_str(&content).unwrap();
        assert!(json["mcp"]["graphmind"].is_null(), "graphmind should be gone after round-trip");
        assert_eq!(
            json["mcp"]["canary-server"]["type"].as_str(),
            Some("local"),
            "canary opencode server should survive round-trip"
        );

        // --- CLAUDE.md block (install_claude_md_block / uninstall_claude_md_block) ---
        let claude_md = claude_dir.join("CLAUDE.md");
        let canary_md = "# Canary heading\nCanary paragraph that must survive.\n";
        fs::write(&claude_md, canary_md).unwrap();

        install_claude_md_block();
        uninstall_claude_md_block();

        let final_md = fs::read_to_string(&claude_md).unwrap();
        assert!(
            final_md.contains("Canary paragraph that must survive."),
            "canary CLAUDE.md content should survive round-trip, got:\n{final_md}"
        );
        assert!(
            !final_md.contains("<!-- GM:START -->"),
            "GM block should be gone after round-trip, got:\n{final_md}"
        );
    });
}

//! E2E tests for the graphmind block in ~/.claude/CLAUDE.md (`install_claude_md_block`): written,
//! updated from a stock block, left alone when edited or unreadable, backed up when it changes.
//!
//! Each test points `HOME` at a fresh temp dir; tests are serialized because `HOME` is
//! process-wide. Run with:
//!   cargo test -p graphmind-cli --test setup_claude_md_e2e -- --test-threads=1

mod common;

use common::{backups_of, with_home};
use graphmind_cli::commands::setup::{install_claude_md_block, Outcome};
use graphmind_config::claude_home::ClaudeHome;
use std::fs;

// ---------------------------------------------------------------------------
// 1. Install — written
// ---------------------------------------------------------------------------

#[test]
fn setup_claude_md_block_written() {
    with_home(|home| {
        let claude_dir = home.join(".claude");
        fs::create_dir_all(&claude_dir).unwrap();

        assert_eq!(install_claude_md_block(&ClaudeHome::current()), Outcome::Written);

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
// 2. Install — replaces a stock block from an older version
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

        assert_eq!(install_claude_md_block(&ClaudeHome::current()), Outcome::Written);

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
        install_claude_md_block(&ClaudeHome::current());
        let installed = fs::read_to_string(&claude_md).unwrap();
        assert!(installed.contains("<!-- GM:HASH:"));
        assert_eq!(install_claude_md_block(&ClaudeHome::current()), Outcome::Unchanged);
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
        assert!(matches!(install_claude_md_block(&ClaudeHome::current()), Outcome::LeftAlone(_)));
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
            assert!(matches!(install_claude_md_block(&ClaudeHome::current()), Outcome::LeftAlone(_)));
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
        if common::skip_as_root("setup_leaves_an_unreadable_claude_md_alone") {
            return;
        }

        assert!(matches!(install_claude_md_block(&ClaudeHome::current()), Outcome::LeftAlone(_)));

        fs::set_permissions(&claude_md, fs::Permissions::from_mode(0o644)).unwrap();
        assert_eq!(fs::read_to_string(&claude_md).unwrap(), "# My own instructions\n");
        assert_eq!(backups_of(&claude_md), 0);
    });
}

// ---------------------------------------------------------------------------
// 3. Install — backup created
// ---------------------------------------------------------------------------

#[test]
fn setup_claude_md_block_backup_created() {
    with_home(|home| {
        let claude_dir = home.join(".claude");
        fs::create_dir_all(&claude_dir).unwrap();
        let claude_md = claude_dir.join("CLAUDE.md");

        fs::write(&claude_md, "# Existing CLAUDE.md content\n").unwrap();

        install_claude_md_block(&ClaudeHome::current());

        // A backup file with .bak extension should have been created.
        assert_eq!(backups_of(&claude_md), 1, "a .bak backup file should be created in ~/.claude/");
    });
}

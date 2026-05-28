mod common;
use common::QueryTestEnv;
use std::fs;

#[test]
fn sync_creates_claude_md_if_missing() {
    let env = QueryTestEnv::new();
    let tmp_dir = tempfile::tempdir().unwrap();
    env.run_ok(&["sync", &env.slug, "--dir", tmp_dir.path().to_str().unwrap()]);

    let claude_md = tmp_dir.path().join("CLAUDE.md");
    assert!(claude_md.exists(), "CLAUDE.md should be created by sync");
    let content = fs::read_to_string(&claude_md).unwrap();
    assert!(content.contains("<!-- GM:START -->"), "CLAUDE.md should contain GM:START, got: {content}");
    assert!(content.contains("<!-- GM:END -->"), "CLAUDE.md should contain GM:END");
}

#[test]
fn sync_updates_claude_md_markers() {
    let env = QueryTestEnv::new();
    let tmp_dir = tempfile::tempdir().unwrap();
    let claude_md = tmp_dir.path().join("CLAUDE.md");

    let initial = "# My Project\n\n<!-- GM:START -->\nold content\n<!-- GM:END -->\n\nother notes\n";
    fs::write(&claude_md, initial).unwrap();

    env.run_ok(&["sync", &env.slug, "--dir", tmp_dir.path().to_str().unwrap()]);

    let content = fs::read_to_string(&claude_md).unwrap();
    let start_count = content.matches("<!-- GM:START -->").count();
    assert_eq!(start_count, 1, "GM:START should appear exactly once after sync");
    assert!(!content.contains("old content"), "old GM block should be replaced");
    assert!(content.contains("other notes"), "content outside GM block should be preserved");
}

#[test]
fn sync_all_flag() {
    let env = QueryTestEnv::new();
    // Register a writable project dir so --all can write CLAUDE.md there
    let tmp_dir = tempfile::tempdir().unwrap();
    env.run_ok(&["register", tmp_dir.path().to_str().unwrap(), "--slug", "sync-all-proj"]);
    env.run_ok(&["build", "sync-all-proj"]);

    let out = env.run(&["sync", "--all"]);
    assert!(out.status.success(), "sync --all should exit 0, stderr: {}", String::from_utf8_lossy(&out.stderr));
}

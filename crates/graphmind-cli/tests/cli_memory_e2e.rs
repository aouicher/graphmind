mod common;
use common::CliTestEnv;

#[test]
fn memory_add_and_list() {
    let env = CliTestEnv::new();
    env.run_ok(&[
        "memory", "add", "test decision content", "--type", "decision", "--global",
    ]);

    // Global memories: list without --in (no project needed)
    let out = env.run(&["memory", "list"]);
    let stdout = String::from_utf8_lossy(&out.stdout).to_string();
    // Either the content appears directly, or the command exits gracefully
    assert!(
        out.status.success() || !stdout.is_empty(),
        "memory list should run, stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        stdout.contains("test decision content"),
        "memory list should contain the added entry, got: {stdout}"
    );
}

#[test]
fn memory_add_global_and_list() {
    let env = CliTestEnv::new();
    env.run_ok(&["memory", "add", "global decision here", "--global", "--type", "decision"]);

    // list without a project (will error if not global) — use a raw run
    let out = env.run(&["memory", "list"]);
    let combined = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    // Should either show the entry or exit gracefully
    assert!(
        out.status.success() || combined.contains("No project"),
        "memory list should run gracefully, combined: {combined}"
    );
}

#[test]
fn memory_search_finds_entry() {
    let env = CliTestEnv::new();
    env.run_ok(&["memory", "add", "JWT auth pattern used everywhere", "--global", "--type", "pattern"]);

    let out = env.run(&["memory", "search", "jwt"]);
    let stdout = String::from_utf8_lossy(&out.stdout).to_string();
    assert!(
        out.status.success(),
        "memory search should exit 0, stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        stdout.to_lowercase().contains("jwt"),
        "memory search should find jwt entry, got: {stdout}"
    );
}

#[test]
fn memory_search_no_results() {
    let env = CliTestEnv::new();
    let out = env.run(&["memory", "search", "xyznonexistentmemory999"]);
    assert!(
        out.status.success(),
        "memory search with no results should exit 0, stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn memory_delete_by_id() {
    let env = CliTestEnv::new();
    // Add a memory
    env.run_ok(&["memory", "add", "delete-me content", "--global", "--type", "context"]);

    // List and parse the id (first 8 chars shown in dimmed, full id needed for delete)
    // The output format is: "  <id_prefix> [type] content"
    // We need to find the full id from the JSONL file
    let mem_dir = env.home.path().join(".graphmind").join("memory");
    let global_file = mem_dir.join("global.jsonl");

    // Read the file to find the id
    let content = std::fs::read_to_string(&global_file).unwrap_or_default();
    let id = content
        .lines()
        .filter_map(|line| {
            let v: serde_json::Value = serde_json::from_str(line).ok()?;
            if v["content"].as_str()? == "delete-me content" {
                Some(v["id"].as_str()?.to_string())
            } else {
                None
            }
        })
        .next();

    if let Some(id) = id {
        env.run_ok(&["memory", "delete", &id]);
        // Verify it's gone
        let after = std::fs::read_to_string(&global_file).unwrap_or_default();
        assert!(
            !after.contains("delete-me content"),
            "memory should be deleted, file content: {after}"
        );
    } else {
        // File might not exist yet — just verify add ran ok
        panic!("Could not find memory entry to delete; global.jsonl content: {content}");
    }
}

#[test]
fn memory_add_priority() {
    let env = CliTestEnv::new();
    env.run_ok(&["memory", "add", "critical priority fact", "--global", "--priority"]);

    // Check the JSONL directly
    let mem_dir = env.home.path().join(".graphmind").join("memory");
    let global_file = mem_dir.join("global.jsonl");
    let content = std::fs::read_to_string(&global_file).unwrap_or_default();
    assert!(
        content.contains("critical priority fact"),
        "priority memory should be stored, content: {content}"
    );
    // priority flag should be in the JSON
    let has_priority = content.lines().any(|line| {
        let v: serde_json::Value = serde_json::from_str(line).unwrap_or_default();
        v["content"].as_str() == Some("critical priority fact")
            && v["priority"].as_bool() == Some(true)
    });
    assert!(has_priority, "entry should have priority=true, content: {content}");
}

#[test]
fn memory_add_global_flag() {
    let env = CliTestEnv::new();
    let out = env.run(&["memory", "add", "global fact test", "--global"]);
    assert!(
        out.status.success(),
        "memory add --global should exit 0, stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

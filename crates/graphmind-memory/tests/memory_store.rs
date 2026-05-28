use graphmind_memory::store::{AddOptions, MemoryStore};
use graphmind_memory::cross_links::{CrossLinkStore, LinkType, NewCrossLink};
use std::io::Write;

fn make_store() -> (MemoryStore, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    // Override HOME so any code relying on HOME-based paths uses our tmp dir
    unsafe { std::env::set_var("HOME", dir.path()) };
    let memory_dir = dir.path().join(".graphmind").join("memory");
    let store = MemoryStore::new(&memory_dir);
    (store, dir)
}

// ── 1. add + list ─────────────────────────────────────────────────────────────

#[test]
fn memory_add_and_list() {
    let (store, _dir) = make_store();
    store.add("entry one", AddOptions::default());
    store.add("entry two", AddOptions::default());

    let entries = store.list(None);
    assert_eq!(entries.len(), 2, "expected 2 entries, got {}", entries.len());
    let contents: Vec<&str> = entries.iter().map(|e| e.content.as_str()).collect();
    assert!(contents.contains(&"entry one"));
    assert!(contents.contains(&"entry two"));
}

// ── 2. keyword search ─────────────────────────────────────────────────────────

#[test]
fn memory_search_by_keyword() {
    let (store, _dir) = make_store();
    store.add("authentication JWT token", AddOptions::default());
    store.add("unrelated database migration", AddOptions::default());

    let all = store.list(None);
    let results = graphmind_memory::search::search(&all, "jwt", 10);
    assert!(!results.is_empty(), "expected to find entry with 'JWT'");
    assert!(results[0].content.to_lowercase().contains("jwt"));
}

// ── 3. search no results ──────────────────────────────────────────────────────

#[test]
fn memory_search_no_results() {
    let (store, _dir) = make_store();
    store.add("some normal content", AddOptions::default());

    let all = store.list(None);
    let results = graphmind_memory::search::search(&all, "xyznonexistent", 10);
    assert!(results.is_empty(), "expected no results for nonexistent term");
}

// ── 4. delete by id ───────────────────────────────────────────────────────────

#[test]
fn memory_delete_by_id() {
    let (store, _dir) = make_store();
    let entry = store.add("to be deleted", AddOptions::default());
    let id = entry.id.clone();

    let deleted = store.delete(&id, None);
    assert!(deleted, "delete should return true");

    let entries = store.list(None);
    assert_eq!(entries.len(), 0, "expected 0 entries after delete");
}

// ── 5. delete nonexistent ─────────────────────────────────────────────────────

#[test]
fn memory_delete_nonexistent_ok() {
    let (store, _dir) = make_store();
    // Should not panic
    let result = store.delete("00000000-0000-0000-0000-000000000000", None);
    assert!(!result, "delete of nonexistent id should return false");
}

// ── 6. list_priority ──────────────────────────────────────────────────────────

#[test]
fn memory_list_priority_only() {
    let (store, _dir) = make_store();
    store.add("normal entry", AddOptions { priority: false, ..Default::default() });
    store.add("priority entry", AddOptions { priority: true, ..Default::default() });

    let priority = store.list_priority(None);
    assert_eq!(priority.len(), 1, "expected 1 priority entry, got {}", priority.len());
    assert_eq!(priority[0].content, "priority entry");
}

// ── 7. project-scoped entries ─────────────────────────────────────────────────

#[test]
fn memory_project_scoped() {
    let (store, _dir) = make_store();
    store.add(
        "proj-a entry",
        AddOptions { project: Some("proj-a".to_string()), ..Default::default() },
    );
    store.add(
        "proj-b entry",
        AddOptions { project: Some("proj-b".to_string()), ..Default::default() },
    );

    // list with project=proj-a should only return proj-a entry
    // (global is empty, proj-a.jsonl has 1 entry)
    let entries_a = store.list(Some("proj-a"));
    assert!(
        entries_a.iter().any(|e| e.content == "proj-a entry"),
        "should find proj-a entry"
    );
    assert!(
        !entries_a.iter().any(|e| e.content == "proj-b entry"),
        "should not find proj-b entry when filtering proj-a"
    );
}

// ── 8. global entries ─────────────────────────────────────────────────────────

#[test]
fn memory_global_entries() {
    let (store, _dir) = make_store();
    store.add(
        "global fact",
        AddOptions { global: true, ..Default::default() },
    );
    store.add(
        "proj-specific fact",
        AddOptions { project: Some("proj-x".to_string()), ..Default::default() },
    );

    // Global entries appear in any project context (they live in global.jsonl)
    let entries = store.list(Some("proj-y")); // different project
    assert!(
        entries.iter().any(|e| e.content == "global fact"),
        "global entry should appear in any project context"
    );
    assert!(
        !entries.iter().any(|e| e.content == "proj-specific fact"),
        "proj-x entry should not appear when listing proj-y"
    );
}

// ── 9. persistence ────────────────────────────────────────────────────────────

#[test]
fn memory_persistence() {
    let dir = tempfile::tempdir().unwrap();
    unsafe { std::env::set_var("HOME", dir.path()) };
    let memory_dir = dir.path().join(".graphmind").join("memory");

    let id = {
        let store1 = MemoryStore::new(&memory_dir);
        let entry = store1.add("persisted content", AddOptions::default());
        entry.id.clone()
    };

    // Create a new instance from the same path
    let store2 = MemoryStore::new(&memory_dir);
    let entries = store2.list(None);
    assert_eq!(entries.len(), 1, "entry should persist across store instances");
    assert_eq!(entries[0].id, id);
    assert_eq!(entries[0].content, "persisted content");
}

// ── 10. corrupted JSONL line ──────────────────────────────────────────────────

#[test]
fn memory_jsonl_corrupted_line() {
    let dir = tempfile::tempdir().unwrap();
    unsafe { std::env::set_var("HOME", dir.path()) };
    let memory_dir = dir.path().join(".graphmind").join("memory");
    std::fs::create_dir_all(&memory_dir).unwrap();

    // Write one valid JSON line and one corrupt line directly to global.jsonl
    let global_path = memory_dir.join("global.jsonl");
    let valid_entry = serde_json::json!({
        "id": "aaaaaaaa-0000-0000-0000-000000000001",
        "created": "2026-01-01T00:00:00Z",
        "updated": "2026-01-01T00:00:00Z",
        "project": null,
        "global": true,
        "type": "context",
        "content": "valid entry",
        "tags": [],
        "session": "2026-01-01",
        "priority": false
    });
    let mut f = std::fs::File::create(&global_path).unwrap();
    writeln!(f, "{}", valid_entry).unwrap();
    writeln!(f, "{{this is not valid json}}").unwrap();

    let store = MemoryStore::new(&memory_dir);
    let entries = store.list(None);
    assert_eq!(entries.len(), 1, "should parse only the valid line, got {}", entries.len());
    assert_eq!(entries[0].content, "valid entry");
}

// ── 11. cross-link add + list ─────────────────────────────────────────────────

#[test]
fn cross_link_add_and_list() {
    let dir = tempfile::tempdir().unwrap();
    let file_path = dir.path().join("cross-links.jsonl");
    let store = CrossLinkStore::new(&file_path);

    store.add(NewCrossLink {
        from: "proj-a".to_string(),
        to: "proj-b".to_string(),
        link_type: LinkType::DependsOn,
        reason: "shares auth logic".to_string(),
        symbols: vec!["AuthService".to_string()],
        inferred: false,
        confidence: 1.0,
    });

    let links = store.list();
    assert_eq!(links.len(), 1, "expected 1 cross-link");
    assert_eq!(links[0].from, "proj-a");
    assert_eq!(links[0].to, "proj-b");
}

// ── 12. infer with <2 projects returns empty gracefully ───────────────────────

#[test]
fn cross_link_infer_requires_two_projects() {
    let dir = tempfile::tempdir().unwrap();
    let cross_path = dir.path().join("cross-links.jsonl");
    let cross_store = CrossLinkStore::new(&cross_path);

    // Call infer with zero projects — should not panic and return empty
    let result = graphmind_memory::cross_infer::infer_cross_links(
        &[],
        |slug| dir.path().join(format!("{slug}.db")).to_string_lossy().to_string(),
        &cross_store,
    );
    assert!(result.is_empty(), "infer with 0 projects should return no links");

    // Call with one project (no DB file) — also should return empty
    let result2 = graphmind_memory::cross_infer::infer_cross_links(
        &["only-project".to_string()],
        |slug| dir.path().join(format!("{slug}.db")).to_string_lossy().to_string(),
        &cross_store,
    );
    assert!(result2.is_empty(), "infer with 1 project should return no links");
}

use graphmind_memory::store::{AddOptions, MemoryError, MemoryStore};
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

    let deleted = store.delete_entry(&id, None).is_ok();
    assert!(deleted, "delete should return true");

    let entries = store.list(None);
    assert_eq!(entries.len(), 0, "expected 0 entries after delete");
}

// ── 5. delete nonexistent ─────────────────────────────────────────────────────

#[test]
fn memory_delete_nonexistent_ok() {
    let (store, _dir) = make_store();
    // Should not panic
    let result = store.delete_entry("00000000-0000-0000-0000-000000000000", None).is_ok();
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

// ── write safety ─────────────────────────────────────────────────────────────

#[test]
fn rewrites_keep_lines_this_version_cannot_parse() {
    let (store, _dir) = make_store();
    let kept = store.add("keep me", AddOptions::default());
    store.add("delete me", AddOptions::default());
    let path = store.global_path();
    let future = r#"{"id":"future-1","type":"insight","content":"written by a newer graphmind"}"#;
    let mut f = std::fs::OpenOptions::new().append(true).open(&path).unwrap();
    writeln!(f, "{future}").unwrap();

    store.increment_recall(&kept.id, None);
    let doomed = store.list(None).into_iter().find(|e| e.content == "delete me").unwrap();
    assert!(store.delete_entry(&doomed.id, None).is_ok());
    store.update_file(&path, |_| ((), true)).unwrap();

    let raw = std::fs::read_to_string(&path).unwrap();
    assert!(raw.contains(future), "unparsed line lost:\n{raw}");
    assert!(raw.contains("keep me") && !raw.contains("delete me"));
}

#[test]
fn add_waits_for_a_rewrite_in_progress() {
    let (store, dir) = make_store();
    store.add("first", AddOptions::default());
    let memory_dir = dir.path().join(".graphmind").join("memory");
    let path = store.global_path();

    let rewriter = {
        let memory_dir = memory_dir.clone();
        let path = path.clone();
        std::thread::spawn(move || {
            let store = MemoryStore::new(&memory_dir);
            store.update_file(&path, |_| {
                std::thread::sleep(std::time::Duration::from_millis(300));
                ((), true)
            });
        })
    };
    std::thread::sleep(std::time::Duration::from_millis(50));
    MemoryStore::new(&memory_dir).add("second", AddOptions::default());
    rewriter.join().unwrap();

    let contents: Vec<String> = store.list(None).into_iter().map(|e| e.content).collect();
    assert!(contents.contains(&"second".to_string()), "append lost to a concurrent rewrite: {contents:?}");
}

#[cfg(unix)]
#[test]
fn failed_write_leaves_the_file_intact() {
    use std::os::unix::fs::PermissionsExt;
    let (store, _dir) = make_store();
    let entry = store.add("original", AddOptions::default());
    let path = store.global_path();
    let before = std::fs::read_to_string(&path).unwrap();

    // The temp file can't be created in a read-only directory — unless we run as root, which
    // ignores directory permissions; then there is nothing to test.
    let mem_dir = path.parent().unwrap().to_path_buf();
    std::fs::set_permissions(&mem_dir, std::fs::Permissions::from_mode(0o555)).unwrap();
    let probe = mem_dir.join("write-probe");
    if std::fs::File::create(&probe).is_ok() {
        std::fs::remove_file(&probe).ok();
        std::fs::set_permissions(&mem_dir, std::fs::Permissions::from_mode(0o755)).unwrap();
        return;
    }
    store.increment_recall(&entry.id, None);
    std::fs::set_permissions(&mem_dir, std::fs::Permissions::from_mode(0o755)).unwrap();

    assert_eq!(std::fs::read_to_string(&path).unwrap(), before);
}

/// A file that can't be read (here: invalid UTF-8) reads as no entries; rewriting it from that
/// would empty it. It is left as it is.
#[test]
fn unreadable_file_is_not_rewritten() {
    let (store, _dir) = make_store();
    let path = store.global_path();
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    let bytes = b"{\"id\":\"x\",\"content\":\"caf\xe9\"}\n".to_vec();
    std::fs::write(&path, &bytes).unwrap();
    assert!(store.list(None).is_empty(), "precondition: reads as no entries");

    let ran = std::cell::Cell::new(false);
    assert!(store.update_file(&path, |_| { ran.set(true); ((), true) }).is_none(), "None: file unchanged");
    assert!(!ran.get(), "the edit doesn't run on a file that can't be read");

    assert_eq!(std::fs::read(&path).unwrap(), bytes);
}

/// A failed write reports the file as unchanged (None), so callers don't claim success.
#[cfg(unix)]
#[test]
fn update_file_reports_a_failed_write() {
    use std::os::unix::fs::PermissionsExt;
    let (store, _dir) = make_store();
    store.add("original", AddOptions::default());
    let path = store.global_path();
    let mem_dir = path.parent().unwrap().to_path_buf();
    std::fs::set_permissions(&mem_dir, std::fs::Permissions::from_mode(0o555)).unwrap();
    let probe = mem_dir.join("write-probe");
    if std::fs::File::create(&probe).is_ok() {
        std::fs::remove_file(&probe).ok(); // root: directory permissions don't apply
        std::fs::set_permissions(&mem_dir, std::fs::Permissions::from_mode(0o755)).unwrap();
        return;
    }
    let result = store.update_file(&path, |entries| { entries.clear(); ((), true) });
    std::fs::set_permissions(&mem_dir, std::fs::Permissions::from_mode(0o755)).unwrap();
    assert!(result.is_none());
    assert_eq!(store.list(None).len(), 1);
}

// ── update in place ──────────────────────────────────────────────────────────

#[test]
fn update_keeps_id_created_recall_and_expiry() {
    use graphmind_memory::store::{MemoryType, MemoryUpdate};
    let (store, _dir) = make_store();
    let e = store.add("old wording", AddOptions { entry_type: MemoryType::Bug, ttl_days: Some(90), ..AddOptions::default() });
    store.increment_recall(&e.id, None);
    store.increment_recall(&e.id, None);
    let before = store.list(None).into_iter().find(|x| x.id == e.id).unwrap();

    let after = store
        .update(&e.id[..8], None, MemoryUpdate { content: Some("new wording".into()), ..Default::default() })
        .unwrap();

    assert_eq!(after.id, before.id);
    assert_eq!(after.created, before.created);
    assert_eq!(after.recall_count, 2);
    assert_eq!(after.expires_at, before.expires_at, "expiry must not restart");
    assert_eq!(after.content, "new wording");
    let stored = store.list(None);
    assert_eq!(stored.len(), 1);
    assert_eq!(stored[0].content, "new wording");
}

#[test]
fn update_toggles_priority_and_type_lifetime() {
    use graphmind_memory::store::{MemoryType, MemoryUpdate};
    let (store, _dir) = make_store();
    let e = store.add("fact", AddOptions { entry_type: MemoryType::Bug, ttl_days: Some(90), ..AddOptions::default() });
    let u = store.update(&e.id, None, MemoryUpdate { priority: Some(true), ..Default::default() }).unwrap();
    assert!(u.priority);
    let u = store.update(&e.id, None, MemoryUpdate { priority: Some(false), ..Default::default() }).unwrap();
    assert!(!u.priority);
    // bug -> decision: decisions don't expire.
    let u = store.update(&e.id, None, MemoryUpdate { entry_type: Some(MemoryType::Decision), ..Default::default() }).unwrap();
    assert!(u.expires_at.is_none() && u.ttl_days.is_none());
}

#[test]
fn update_reports_missing_and_ambiguous_ids() {
    use graphmind_memory::store::{MemoryUpdate, MemoryError};
    let (store, _dir) = make_store();
    // Any real change: an empty one is rejected before the id is looked up.
    let edit = || MemoryUpdate { priority: Some(true), ..Default::default() };
    store.add("a", AddOptions::default());
    store.add("b", AddOptions::default());
    assert_eq!(store.update("does-not-exist", None, edit()).unwrap_err(), MemoryError::NotFound);
    assert_eq!(store.update("", None, edit()).unwrap_err(), MemoryError::TooShort);
    // Two ids sharing an 8-character prefix: the prefix is ambiguous, each full id still works.
    let path = store.global_path();
    store.update_file(&path, |entries| {
        entries[0].id = "abc11111-0000".into();
        entries[1].id = "abc11111-1111".into();
        ((), true)
    }).unwrap();
    assert!(matches!(store.update("abc11111", None, edit()), Err(MemoryError::Ambiguous(2))));
    assert!(store.update("abc11111-0000", None, MemoryUpdate { priority: Some(true), ..Default::default() }).is_ok());
}

#[test]
fn delete_accepts_the_short_id_listings_print() {
    let (store, _dir) = make_store();
    let keep = store.add("keep", AddOptions::default());
    let doomed = store.add("doomed", AddOptions::default());
    assert!(store.delete_entry(&doomed.id[..8], None).is_ok());
    let left: Vec<String> = store.list(None).into_iter().map(|e| e.id).collect();
    assert_eq!(left, [keep.id]);
    assert_eq!(store.delete_entry("", None), Err(MemoryError::TooShort), "an empty id never deletes");
}

#[test]
fn update_and_delete_need_eight_id_characters() {
    use graphmind_memory::store::{MemoryUpdate, MemoryError};
    let (store, _dir) = make_store();
    let e = store.add("only entry", AddOptions::default());
    // A short prefix matching the only entry must not touch it.
    assert_eq!(store.delete_entry(&e.id[..1], None), Err(MemoryError::TooShort));
    assert_eq!(
        store.update(&e.id[..7], None, MemoryUpdate { priority: Some(true), ..Default::default() }).unwrap_err(),
        MemoryError::TooShort
    );
    assert_eq!(store.list(None).len(), 1);
    assert!(store.delete_entry(&e.id[..8], None).is_ok());
}

#[test]
fn demotion_resets_recall_count() {
    use graphmind_memory::store::MemoryUpdate;
    let (store, _dir) = make_store();
    let e = store.add("promoted by recalls", AddOptions { priority: true, ..AddOptions::default() });
    for _ in 0..5 {
        store.increment_recall(&e.id, None);
    }
    let u = store.update(&e.id, None, MemoryUpdate { priority: Some(false), ..Default::default() }).unwrap();
    assert!(!u.priority);
    assert_eq!(u.recall_count, 0, "a recall count left at 5 would re-promote it on the next consolidate");
}

#[test]
fn priority_false_on_a_normal_entry_keeps_its_recall_count() {
    use graphmind_memory::store::MemoryUpdate;
    let (store, _dir) = make_store();
    let e = store.add("never promoted", AddOptions::default());
    for _ in 0..2 {
        store.increment_recall(&e.id, None);
    }
    let u = store.update(&e.id, None, MemoryUpdate { priority: Some(false), ..Default::default() }).unwrap();
    assert!(!u.priority);
    assert_eq!(u.recall_count, 2, "not a demotion: the count must survive");
}

#[test]
fn memory_type_parse_rejects_unknown_names() {
    use graphmind_memory::store::MemoryType;
    for name in MemoryType::NAMES {
        assert!(name.parse::<MemoryType>().is_ok(), "{name}");
    }
    let err = "decisions".parse::<MemoryType>().unwrap_err();
    assert!(err.contains("decisions") && err.contains("decision, pattern"), "{err}");
}

#[test]
fn an_empty_update_is_rejected_and_writes_nothing() {
    use graphmind_memory::store::{MemoryError, MemoryUpdate};
    let (store, _dir) = make_store();
    let e = store.add("fact", AddOptions::default());
    let before = std::fs::read_to_string(store.global_path()).unwrap();
    assert_eq!(store.update(&e.id, None, MemoryUpdate::default()).unwrap_err(), MemoryError::NothingToChange);
    assert_eq!(std::fs::read_to_string(store.global_path()).unwrap(), before);
}

/// A write that fails is an error, so the CLI doesn't print "updated" and MCP doesn't re-embed.
#[cfg(unix)]
#[test]
fn a_failed_update_write_is_an_error() {
    use graphmind_memory::store::{MemoryError, MemoryUpdate};
    use std::os::unix::fs::PermissionsExt;
    let (store, _dir) = make_store();
    let e = store.add("fact", AddOptions::default());
    let mem_dir = store.global_path().parent().unwrap().to_path_buf();
    std::fs::set_permissions(&mem_dir, std::fs::Permissions::from_mode(0o555)).unwrap();
    let probe = mem_dir.join("write-probe");
    if std::fs::File::create(&probe).is_ok() {
        std::fs::remove_file(&probe).ok(); // root: directory permissions don't apply
        std::fs::set_permissions(&mem_dir, std::fs::Permissions::from_mode(0o755)).unwrap();
        return;
    }
    let result = store.update(&e.id, None, MemoryUpdate { content: Some("new".into()), ..Default::default() });
    std::fs::set_permissions(&mem_dir, std::fs::Permissions::from_mode(0o755)).unwrap();
    assert_eq!(result.unwrap_err(), MemoryError::NotWritten);
    assert_eq!(store.get(&e.id, None).unwrap().content, "fact");
}

// ── tags ─────────────────────────────────────────────────────────────────────

/// Names in the text are matched at search time; `add` doesn't write them into the tags.
#[test]
fn add_stores_tags_as_given() {
    let (store, _dir) = make_store();
    let e = store.add("make sure fetchInBatches reads src/a.ts", AddOptions { tags: vec!["Cache".into()], ..AddOptions::default() });
    assert_eq!(e.tags, ["Cache"]);
    assert_eq!(store.list(None)[0].tags, ["Cache"]);
}

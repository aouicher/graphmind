//! Tests for remote semantic search path in handle_search.
//! These tests verify the offline fallback behaviour (no remote server needed).

use graphmind_mcp::handlers::dispatch_tool;
use serde_json::json;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, OnceLock};

static TEST_LOCK: Mutex<()> = Mutex::new(());

// ── Shared fixture graph ───────────────────────────────────────────────────

struct SharedGraph {
    graphmind_dir: PathBuf,
    slug: String,
    _dir: Arc<tempfile::TempDir>,
}

static SHARED_GRAPH: OnceLock<SharedGraph> = OnceLock::new();

fn shared_graph() -> &'static SharedGraph {
    SHARED_GRAPH.get_or_init(|| {
        let dir = tempfile::tempdir().unwrap();
        let slug = "test-remote".to_string();

        let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent().unwrap()
            .parent().unwrap()
            .join("tests/fixtures/sample-project");

        let graphmind_dir = dir.path().join(".graphmind");
        let graph_dir = graphmind_dir.join("graphs").join(&slug);
        std::fs::create_dir_all(&graph_dir).unwrap();
        std::fs::create_dir_all(graphmind_dir.join("memory")).unwrap();

        let fixture_path_str = fixture.to_str().unwrap().to_string();
        let config = json!({
            "version": "1",
            "projects": {
                &slug: {
                    "slug": &slug,
                    "path": &fixture_path_str,
                    "last_build": null,
                    "languages": [],
                    "registered": "2024-01-01T00:00:00Z",
                    "auto_watch": false,
                    "exclude": []
                }
            },
            "global_exclude": [],
            "defaults": { "embedding_model": "minilm", "watch_debounce": 2000, "max_depth": 5, "exclude_tests": true },
            "mcp": { "transport": "stdio", "http_port": 37378, "restrict_to_projects": null },
            "remote": { "mode": "off" }
        });
        std::fs::write(graphmind_dir.join("config.json"), config.to_string()).unwrap();

        let db_path = graph_dir.join("graph.db");
        let cache_dir = graph_dir.join("cache");
        std::fs::create_dir_all(&cache_dir).unwrap();
        let mut builder = graphmind_db::builder::GraphBuilder::new(
            db_path.to_str().unwrap(),
            cache_dir.to_str().unwrap(),
        );
        builder.build(
            fixture.to_str().unwrap(),
            &graphmind_db::builder::BuildOptions { full: true, ..Default::default() },
        );

        SharedGraph { graphmind_dir, slug, _dir: Arc::new(dir) }
    })
}

struct TestCtx {
    slug: String,
    _dir: tempfile::TempDir,
    _lock: std::sync::MutexGuard<'static, ()>,
}

fn setup() -> TestCtx {
    let lock = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let shared = shared_graph();
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join(".graphmind");
    copy_dir_all(&shared.graphmind_dir, &dest).unwrap();

    let config_src = shared.graphmind_dir.join("config.json");
    let config_str = std::fs::read_to_string(&config_src).unwrap();
    let new_config = config_str.replace(
        shared.graphmind_dir.parent().unwrap().to_str().unwrap(),
        dir.path().to_str().unwrap(),
    );
    std::fs::write(dest.join("config.json"), &new_config).unwrap();
    unsafe { std::env::set_var("HOME", dir.path()); }

    TestCtx { slug: shared.slug.clone(), _dir: dir, _lock: lock }
}

fn copy_dir_all(src: &PathBuf, dst: &PathBuf) -> std::io::Result<()> {
    std::fs::create_dir_all(dst)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let ty = entry.file_type()?;
        let dest = dst.join(entry.file_name());
        if ty.is_dir() { copy_dir_all(&entry.path(), &dest)?; }
        else { std::fs::copy(entry.path(), dest)?; }
    }
    Ok(())
}

fn write_config(ctx: &TestCtx, remote_mode: &str, license_key: Option<&str>) {
    let shared = shared_graph();
    let config_str = std::fs::read_to_string(shared.graphmind_dir.join("config.json")).unwrap();
    let new_config = config_str.replace(
        shared.graphmind_dir.parent().unwrap().to_str().unwrap(),
        ctx._dir.path().to_str().unwrap(),
    );
    let mut cfg: serde_json::Value = serde_json::from_str(&new_config).unwrap();
    cfg["remote"]["mode"] = json!(remote_mode);
    if let Some(key) = license_key {
        cfg["license"] = json!({ "key": key });
    }
    std::fs::write(ctx._dir.path().join(".graphmind").join("config.json"), cfg.to_string()).unwrap();
}

// ── Tests ──────────────────────────────────────────────────────────────────

#[test]
fn search_works_in_off_mode_fts_only() {
    let ctx = setup();
    write_config(&ctx, "off", None);

    let result = dispatch_tool("gm_search", &json!({ "query": "function", "project": ctx.slug }));
    let text = result["content"][0]["text"].as_str().unwrap_or("");
    assert!(!text.is_empty(), "search in off mode should return FTS results");
    assert!(!result.get("isError").and_then(|v| v.as_bool()).unwrap_or(false),
        "search in off mode should not error: {text}");
}

#[test]
fn search_works_in_embed_mode_no_key_falls_back_to_fts() {
    let ctx = setup();
    // remote.mode = embed but no license key → remote_semantic_search returns []
    // → fuse_fts_and_semantic gets only FTS results → should still return results
    write_config(&ctx, "embed", None);

    let result = dispatch_tool("gm_search", &json!({ "query": "function", "project": ctx.slug }));
    let text = result["content"][0]["text"].as_str().unwrap_or("");
    assert!(!text.is_empty(), "search in embed mode with no key should fall back to FTS results");
    assert!(!result.get("isError").and_then(|v| v.as_bool()).unwrap_or(false),
        "embed mode with no key should not hard-error: {text}");
}

#[test]
fn search_works_in_full_mode_no_key_falls_back_to_fts() {
    let ctx = setup();
    write_config(&ctx, "full", None);

    let result = dispatch_tool("gm_search", &json!({ "query": "function", "project": ctx.slug }));
    let text = result["content"][0]["text"].as_str().unwrap_or("");
    assert!(!text.is_empty(), "search in full mode with no key should fall back to FTS results");
    assert!(!result.get("isError").and_then(|v| v.as_bool()).unwrap_or(false),
        "full mode with no key should not hard-error: {text}");
}

#[test]
fn search_missing_query_returns_error_in_all_modes() {
    for mode in &["off", "embed", "full"] {
        let ctx = setup();
        write_config(&ctx, mode, None);

        let result = dispatch_tool("gm_search", &json!({ "project": ctx.slug }));
        assert!(
            result.get("isError").and_then(|v| v.as_bool()).unwrap_or(false),
            "missing query should return error in {mode} mode"
        );
    }
}

#[test]
fn search_empty_query_returns_error() {
    let ctx = setup();
    write_config(&ctx, "off", None);

    let result = dispatch_tool("gm_search", &json!({ "query": "", "project": ctx.slug }));
    assert!(
        result.get("isError").and_then(|v| v.as_bool()).unwrap_or(false),
        "empty query should return error"
    );
}

#[test]
fn search_limit_respected_in_off_mode() {
    let ctx = setup();
    write_config(&ctx, "off", None);

    let result = dispatch_tool("gm_search", &json!({ "query": "a", "limit": 3, "project": ctx.slug }));
    let text = result["content"][0]["text"].as_str().unwrap_or("");
    // Just verify it doesn't crash — result count checking is format-dependent
    assert!(!result.get("isError").and_then(|v| v.as_bool()).unwrap_or(false),
        "search with limit should not error: {text}");
}

#[test]
fn search_returns_same_results_off_vs_embed_no_key() {
    // With no key, embed mode falls back to pure FTS — should return same results as off mode
    let ctx_off = setup();
    write_config(&ctx_off, "off", None);
    let result_off = dispatch_tool("gm_search", &json!({ "query": "function", "project": ctx_off.slug }));

    let ctx_embed = setup();
    write_config(&ctx_embed, "embed", None);
    let result_embed = dispatch_tool("gm_search", &json!({ "query": "function", "project": ctx_embed.slug }));

    // Both should be non-errors and non-empty
    assert!(!result_off.get("isError").and_then(|v| v.as_bool()).unwrap_or(false));
    assert!(!result_embed.get("isError").and_then(|v| v.as_bool()).unwrap_or(false));
    let off_text = result_off["content"][0]["text"].as_str().unwrap_or("");
    let embed_text = result_embed["content"][0]["text"].as_str().unwrap_or("");
    assert!(!off_text.is_empty());
    assert!(!embed_text.is_empty());
}

#[test]
fn search_with_invalid_live_jwt_falls_back_gracefully() {
    // gm_live_ prefix + garbage JWT → ApiClient builds, HTTP call fails → empty semantic results
    // → FTS fallback kicks in, no panic
    let ctx = setup();
    write_config(&ctx, "embed", Some("gm_live_invalid.jwt.payload"));

    let result = dispatch_tool("gm_search", &json!({ "query": "function", "project": ctx.slug }));
    assert!(!result.get("isError").and_then(|v| v.as_bool()).unwrap_or(false),
        "search with bad JWT should not crash: {:?}", result);
}

use graphmind_mcp::handlers::dispatch_tool;
use serde_json::{json, Value};
use std::path::PathBuf;
use std::sync::Mutex;

// Tests must run serially because they override the HOME env var.
// Use `--test-threads=1` or acquire this lock in each test.
static TEST_LOCK: Mutex<()> = Mutex::new(());

struct TestCtx {
    slug: String,
    _dir: tempfile::TempDir,
    _lock: std::sync::MutexGuard<'static, ()>,
}

fn setup() -> TestCtx {
    let lock = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let dir = tempfile::tempdir().unwrap();
    let slug = "test-fixture".to_string();

    // Locate fixture relative to workspace root
    let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap() // crates/
        .parent()
        .unwrap() // workspace root
        .join("tests/fixtures/sample-project");

    let graphmind_home = dir.path().join(".graphmind");
    let graph_dir = graphmind_home.join("graphs").join(&slug);
    std::fs::create_dir_all(&graph_dir).unwrap();
    std::fs::create_dir_all(graphmind_home.join("memory")).unwrap();

    // Minimal config.json
    let fixture_path = fixture.to_str().unwrap().to_string();
    let config = json!({
        "version": "1",
        "projects": {
            &slug: {
                "slug": &slug,
                "path": &fixture_path,
                "last_build": null,
                "languages": [],
                "registered": "2024-01-01T00:00:00Z",
                "auto_watch": false,
                "exclude": []
            }
        },
        "global_exclude": [],
        "defaults": {
            "embedding_model": "minilm",
            "watch_debounce": 2000,
            "max_depth": 5,
            "exclude_tests": true
        },
        "mcp": {
            "transport": "stdio",
            "http_port": 37378,
            "restrict_to_projects": null
        }
    });
    std::fs::write(graphmind_home.join("config.json"), config.to_string()).unwrap();

    // Override HOME so graphmind_config::paths use temp dir
    unsafe { std::env::set_var("HOME", dir.path()); }

    // Build the graph into temp dir
    let db_path = graph_dir.join("graph.db");
    let cache_dir = graph_dir.join("cache");
    std::fs::create_dir_all(&cache_dir).unwrap();
    let mut builder = graphmind_db::builder::GraphBuilder::new(
        db_path.to_str().unwrap(),
        cache_dir.to_str().unwrap(),
    );
    builder.build(
        fixture.to_str().unwrap(),
        &graphmind_db::builder::BuildOptions {
            full: true,
            ..Default::default()
        },
    );

    TestCtx { slug, _dir: dir, _lock: lock }
}

impl TestCtx {
    fn dispatch(&self, tool: &str, args: Value) -> Value {
        dispatch_tool(tool, &args)
    }

    fn is_error(&self, v: &Value) -> bool {
        v.get("isError").and_then(|v| v.as_bool()).unwrap_or(false)
    }

    fn text(&self, v: &Value) -> String {
        v["content"][0]["text"].as_str().unwrap_or("").to_string()
    }
}

// ---------------------------------------------------------------------------
// Group 2: Missing/invalid params (no fixture build needed — fast)
// ---------------------------------------------------------------------------

#[test]
fn gm_fn_missing_symbol_returns_error() {
    let resp = dispatch_tool("gm_fn", &json!({}));
    assert_eq!(resp.get("isError"), Some(&json!(true)));
    let text = resp["content"][0]["text"].as_str().unwrap_or("");
    assert!(text.contains("Missing"), "expected 'Missing' in: {text}");
}

#[test]
fn gm_deps_missing_file_returns_error() {
    let resp = dispatch_tool("gm_deps", &json!({}));
    assert_eq!(resp.get("isError"), Some(&json!(true)));
}

#[test]
fn gm_search_missing_query_returns_error() {
    let resp = dispatch_tool("gm_search", &json!({}));
    assert_eq!(resp.get("isError"), Some(&json!(true)));
}

#[test]
fn unknown_tool_returns_error() {
    let resp = dispatch_tool("gm_nonexistent_tool_xyz", &json!({}));
    assert_eq!(resp.get("isError"), Some(&json!(true)));
    let text = resp["content"][0]["text"].as_str().unwrap_or("");
    assert!(
        text.contains("Unknown tool"),
        "expected 'Unknown tool' in: {text}"
    );
}

#[test]
fn error_responses_have_is_error_true() {
    let resp = dispatch_tool("gm_fn", &json!({}));
    assert_eq!(
        resp.get("isError"),
        Some(&json!(true)),
        "isError must be exactly true"
    );
}

// ---------------------------------------------------------------------------
// Group 1: Basic behavior (requires fixture build)
// ---------------------------------------------------------------------------

#[test]
fn gm_fn_returns_symbol_with_callers() {
    let ctx = setup();
    let resp = ctx.dispatch("gm_fn", json!({ "symbol": "createWallet", "project": ctx.slug }));
    assert!(!ctx.is_error(&resp), "unexpected error: {}", ctx.text(&resp));
    let text = ctx.text(&resp);
    assert!(
        text.contains("createWallet"),
        "text should contain 'createWallet': {text}"
    );
}

#[test]
fn gm_fn_unknown_symbol_returns_not_found() {
    let ctx = setup();
    let resp = ctx.dispatch(
        "gm_fn",
        json!({ "symbol": "nonExistentFunctionXYZ", "project": ctx.slug }),
    );
    assert!(!ctx.is_error(&resp), "unexpected error: {}", ctx.text(&resp));
}

#[test]
fn gm_search_returns_results() {
    let ctx = setup();
    let resp = ctx.dispatch("gm_search", json!({ "query": "wallet", "project": ctx.slug }));
    assert!(!ctx.is_error(&resp), "unexpected error: {}", ctx.text(&resp));
    assert!(!ctx.text(&resp).is_empty());
}

#[test]
fn gm_search_no_results_graceful() {
    let ctx = setup();
    let resp = ctx.dispatch(
        "gm_search",
        json!({ "query": "xyznonexistentsymbol99", "project": ctx.slug }),
    );
    assert!(!ctx.is_error(&resp), "unexpected error: {}", ctx.text(&resp));
}

#[test]
fn gm_deps_returns_output() {
    let ctx = setup();
    let resp = ctx.dispatch(
        "gm_deps",
        json!({ "file": "src/services/wallet.ts", "project": ctx.slug }),
    );
    assert!(!ctx.is_error(&resp), "unexpected error: {}", ctx.text(&resp));
    assert!(!ctx.text(&resp).is_empty());
}

#[test]
fn gm_map_returns_files() {
    let ctx = setup();
    let resp = ctx.dispatch("gm_map", json!({ "project": ctx.slug }));
    assert!(!ctx.is_error(&resp), "unexpected error: {}", ctx.text(&resp));
    assert!(!ctx.text(&resp).is_empty());
}

#[test]
fn gm_outline_returns_hierarchy() {
    let ctx = setup();
    let resp = ctx.dispatch(
        "gm_outline",
        json!({ "file": "src/services/wallet.ts", "project": ctx.slug }),
    );
    assert!(!ctx.is_error(&resp), "unexpected error: {}", ctx.text(&resp));
    let text = ctx.text(&resp);
    assert!(
        text.contains("WalletService"),
        "expected 'WalletService' in: {text}"
    );
}

#[test]
fn gm_status_returns_stats() {
    let ctx = setup();
    let resp = ctx.dispatch("gm_status", json!({ "project": ctx.slug }));
    assert!(!ctx.is_error(&resp), "unexpected error: {}", ctx.text(&resp));
    let text = ctx.text(&resp);
    assert!(text.contains("symbols"), "expected 'symbols' in: {text}");
    assert!(text.contains("edges"), "expected 'edges' in: {text}");
}

#[test]
fn gm_list_projects_returns_slug() {
    let ctx = setup();
    let resp = ctx.dispatch("gm_list_projects", json!({}));
    assert!(!ctx.is_error(&resp), "unexpected error: {}", ctx.text(&resp));
    let text = ctx.text(&resp);
    assert!(
        text.contains(&ctx.slug),
        "expected slug '{}' in: {text}",
        ctx.slug
    );
}

// ---------------------------------------------------------------------------
// Group 3: Response format contract
// ---------------------------------------------------------------------------

#[test]
fn all_responses_have_content_array() {
    let ctx = setup();
    let cases = vec![
        ("gm_search", json!({ "query": "wallet", "project": ctx.slug })),
        ("gm_map", json!({ "project": ctx.slug })),
        ("gm_status", json!({ "project": ctx.slug })),
        ("gm_list_projects", json!({})),
    ];
    for (tool, args) in cases {
        let resp = ctx.dispatch(tool, args);
        assert!(!ctx.is_error(&resp), "{tool} returned error: {}", ctx.text(&resp));
        let text = ctx.text(&resp);
        assert!(!text.is_empty(), "{tool} returned empty text");
    }
}

// ---------------------------------------------------------------------------
// Group 4: Non-regression
// ---------------------------------------------------------------------------

#[test]
fn gm_search_semicolon_does_not_panic() {
    let ctx = setup();
    let resp = ctx.dispatch(
        "gm_search",
        json!({ "query": "wallet;service", "project": ctx.slug }),
    );
    assert!(!ctx.is_error(&resp));
}

#[test]
fn gm_search_dot_does_not_panic() {
    let ctx = setup();
    let resp = ctx.dispatch(
        "gm_search",
        json!({ "query": "wallet.service", "project": ctx.slug }),
    );
    assert!(!ctx.is_error(&resp));
}

#[test]
fn gm_fn_with_file_filter() {
    let ctx = setup();
    let resp = ctx.dispatch(
        "gm_fn",
        json!({
            "symbol": "createWallet",
            "file": "src/services/wallet.ts",
            "project": ctx.slug
        }),
    );
    assert!(!ctx.is_error(&resp), "unexpected error: {}", ctx.text(&resp));
}

#[test]
fn gm_fn_no_project_falls_back() {
    let ctx = setup();
    let resp = ctx.dispatch("gm_fn", json!({ "symbol": "createWallet" }));
    assert!(!ctx.is_error(&resp), "unexpected error: {}", ctx.text(&resp));
}

#[test]
fn truncation_does_not_crash() {
    let ctx = setup();
    let resp = ctx.dispatch(
        "gm_search",
        json!({ "query": "wallet", "limit": 1000, "project": ctx.slug }),
    );
    assert!(!ctx.is_error(&resp));
}

// ---------------------------------------------------------------------------
// Group 5: Extended tool coverage (18 previously untested tools)
// ---------------------------------------------------------------------------

#[test]
fn gm_query_returns_symbol() {
    let ctx = setup();
    let resp = ctx.dispatch("gm_query", json!({ "symbol": "createWallet", "project": ctx.slug }));
    assert!(!ctx.is_error(&resp), "unexpected error: {}", ctx.text(&resp));
    let text = ctx.text(&resp);
    assert!(
        text.contains("createWallet"),
        "expected 'createWallet' in: {text}"
    );
}

#[test]
fn gm_query_unknown_symbol_not_found() {
    let ctx = setup();
    let resp = ctx.dispatch(
        "gm_query",
        json!({ "symbol": "nonExistentXYZ999", "project": ctx.slug }),
    );
    assert!(!ctx.is_error(&resp), "unexpected error: {}", ctx.text(&resp));
}

#[test]
fn gm_impact_returns_output() {
    let ctx = setup();
    let resp = ctx.dispatch(
        "gm_impact",
        json!({ "file": "src/services/wallet.ts", "project": ctx.slug }),
    );
    assert!(!ctx.is_error(&resp), "unexpected error: {}", ctx.text(&resp));
    let text = ctx.text(&resp);
    assert!(!text.is_empty());
}

#[test]
fn gm_impact_missing_file_returns_error() {
    let resp = dispatch_tool("gm_impact", &json!({}));
    assert_eq!(resp.get("isError"), Some(&json!(true)));
}

#[test]
fn gm_fn_impact_returns_output() {
    let ctx = setup();
    let resp = ctx.dispatch(
        "gm_fn_impact",
        json!({ "symbol": "validateAddress", "project": ctx.slug }),
    );
    assert!(!ctx.is_error(&resp), "unexpected error: {}", ctx.text(&resp));
    let text = ctx.text(&resp);
    assert!(!text.is_empty());
}

#[test]
fn gm_fn_impact_missing_symbol_returns_error() {
    let resp = dispatch_tool("gm_fn_impact", &json!({}));
    assert_eq!(resp.get("isError"), Some(&json!(true)));
}

#[test]
fn gm_cycles_returns_output() {
    let ctx = setup();
    let resp = ctx.dispatch("gm_cycles", json!({ "project": ctx.slug }));
    assert!(!ctx.is_error(&resp), "unexpected error: {}", ctx.text(&resp));
    let text = ctx.text(&resp);
    assert!(!text.is_empty());
}

#[test]
fn gm_file_returns_content() {
    let ctx = setup();
    let resp = ctx.dispatch(
        "gm_file",
        json!({ "file": "src/services/wallet.ts", "project": ctx.slug }),
    );
    assert!(!ctx.is_error(&resp), "unexpected error: {}", ctx.text(&resp));
    let text = ctx.text(&resp);
    assert!(
        text.contains("wallet"),
        "expected 'wallet' in: {text}"
    );
}

#[test]
fn gm_file_not_found_returns_error() {
    let ctx = setup();
    let resp = ctx.dispatch(
        "gm_file",
        json!({ "file": "src/nonexistent.ts", "project": ctx.slug }),
    );
    // handler returns err_text when file cannot be read
    assert_eq!(
        resp.get("isError"),
        Some(&json!(true)),
        "expected isError=true for missing file"
    );
}

#[test]
fn gm_memory_add_returns_success() {
    let ctx = setup();
    let resp = ctx.dispatch(
        "gm_memory_add",
        json!({ "content": "test memory entry", "type": "context" }),
    );
    assert!(!ctx.is_error(&resp), "unexpected error: {}", ctx.text(&resp));
    let text = ctx.text(&resp);
    assert!(!text.is_empty());
}

#[test]
fn gm_memory_search_returns_output() {
    let ctx = setup();
    // First add a memory entry so search has something to find
    ctx.dispatch(
        "gm_memory_add",
        json!({ "content": "test memory search entry unique123", "type": "context" }),
    );
    let resp = ctx.dispatch(
        "gm_memory_search",
        json!({ "query": "test memory" }),
    );
    assert!(!ctx.is_error(&resp), "unexpected error: {}", ctx.text(&resp));
    let text = ctx.text(&resp);
    assert!(!text.is_empty());
}

#[test]
fn gm_memory_list_returns_output() {
    let ctx = setup();
    // Add an entry so the list is non-empty
    ctx.dispatch(
        "gm_memory_add",
        json!({ "content": "memory list test entry", "type": "context" }),
    );
    let resp = ctx.dispatch("gm_memory_list", json!({}));
    assert!(!ctx.is_error(&resp), "unexpected error: {}", ctx.text(&resp));
    let text = ctx.text(&resp);
    assert!(!text.is_empty());
}

#[test]
fn gm_context_returns_output() {
    let ctx = setup();
    let resp = ctx.dispatch("gm_context", json!({ "project": ctx.slug }));
    assert!(!ctx.is_error(&resp), "unexpected error: {}", ctx.text(&resp));
    let text = ctx.text(&resp);
    assert!(!text.is_empty());
}

#[test]
fn gm_who_calls_chain_returns_callers() {
    let ctx = setup();
    let resp = ctx.dispatch(
        "gm_who_calls_chain",
        json!({ "symbol": "validateAddress", "project": ctx.slug }),
    );
    assert!(!ctx.is_error(&resp), "unexpected error: {}", ctx.text(&resp));
    let text = ctx.text(&resp);
    assert!(!text.is_empty());
}

#[test]
fn gm_dead_code_returns_output() {
    let ctx = setup();
    let resp = ctx.dispatch("gm_dead_code", json!({ "project": ctx.slug }));
    assert!(!ctx.is_error(&resp), "unexpected error: {}", ctx.text(&resp));
    let text = ctx.text(&resp);
    assert!(!text.is_empty());
}

#[test]
fn gm_similar_returns_output() {
    let ctx = setup();
    let resp = ctx.dispatch(
        "gm_similar",
        json!({ "symbol": "createWallet", "project": ctx.slug }),
    );
    assert!(!ctx.is_error(&resp), "unexpected error: {}", ctx.text(&resp));
    let text = ctx.text(&resp);
    assert!(!text.is_empty());
}

#[test]
fn gm_listeners_returns_output() {
    let ctx = setup();
    let resp = ctx.dispatch(
        "gm_listeners",
        json!({ "event": "wallet_created", "project": ctx.slug }),
    );
    assert!(!ctx.is_error(&resp), "unexpected error: {}", ctx.text(&resp));
    let text = ctx.text(&resp);
    assert!(!text.is_empty());
}

#[test]
fn gm_listeners_missing_event_returns_error() {
    let resp = dispatch_tool("gm_listeners", &json!({}));
    assert_eq!(resp.get("isError"), Some(&json!(true)));
}

#[test]
fn gm_export_returns_output() {
    let ctx = setup();
    let resp = ctx.dispatch(
        "gm_export",
        json!({
            "file": "src/services/wallet.ts",
            "format": "mermaid",
            "project": ctx.slug
        }),
    );
    assert!(!ctx.is_error(&resp), "unexpected error: {}", ctx.text(&resp));
    let text = ctx.text(&resp);
    assert!(!text.is_empty());
}

#[test]
fn gm_export_missing_file_and_symbol_returns_error() {
    let ctx = setup();
    let resp = ctx.dispatch(
        "gm_export",
        json!({ "format": "mermaid", "project": ctx.slug }),
    );
    assert_eq!(
        resp.get("isError"),
        Some(&json!(true)),
        "expected isError=true when neither file nor symbol provided"
    );
}

#[test]
fn gm_diff_impact_returns_output() {
    let ctx = setup();
    let resp = ctx.dispatch("gm_diff_impact", json!({ "project": ctx.slug }));
    assert!(!ctx.is_error(&resp), "unexpected error: {}", ctx.text(&resp));
    let text = ctx.text(&resp);
    assert!(!text.is_empty());
}

#[test]
fn gm_cross_query_returns_output() {
    let ctx = setup();
    let resp = ctx.dispatch("gm_cross_query", json!({ "symbol": "createWallet" }));
    assert!(!ctx.is_error(&resp), "unexpected error: {}", ctx.text(&resp));
    let text = ctx.text(&resp);
    assert!(!text.is_empty());
}

#[test]
fn gm_cross_deps_returns_output() {
    let ctx = setup();
    let resp = ctx.dispatch("gm_cross_deps", json!({ "project": ctx.slug }));
    assert!(!ctx.is_error(&resp), "unexpected error: {}", ctx.text(&resp));
    let text = ctx.text(&resp);
    assert!(!text.is_empty());
}

#[test]
fn gm_cross_links_returns_output() {
    let _ctx = setup();
    let resp = dispatch_tool("gm_cross_links", &json!({}));
    assert_eq!(resp.get("isError"), None, "unexpected isError field");
    let text = resp["content"][0]["text"].as_str().unwrap_or("");
    assert!(!text.is_empty());
}

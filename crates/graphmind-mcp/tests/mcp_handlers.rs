use graphmind_mcp::handlers::dispatch_tool;
use serde_json::{json, Value};

mod common;

use common::{setup, TestCtx};

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
fn gm_fn_impact_file_filter_narrows_ambiguous_symbol() {
    // "fetch_balance" is defined identically in wallet.rs and wallet.rb in the
    // fixture. Without `file`, callers from both definitions are merged;
    // with `file`, only callers of the file-scoped definition are returned.
    let ctx = setup();
    let unfiltered = ctx.dispatch(
        "gm_fn_impact",
        json!({ "symbol": "fetch_balance", "project": ctx.slug }),
    );
    assert!(!ctx.is_error(&unfiltered), "unexpected error: {}", ctx.text(&unfiltered));
    let unfiltered_json: Value = serde_json::from_str(&ctx.text(&unfiltered)).unwrap();
    let unfiltered_count = unfiltered_json["caller_count"].as_i64().unwrap();

    let filtered = ctx.dispatch(
        "gm_fn_impact",
        json!({
            "symbol": "fetch_balance",
            "file": "src/services/wallet.rs",
            "project": ctx.slug
        }),
    );
    assert!(!ctx.is_error(&filtered), "unexpected error: {}", ctx.text(&filtered));
    let filtered_json: Value = serde_json::from_str(&ctx.text(&filtered)).unwrap();
    let filtered_count = filtered_json["caller_count"].as_i64().unwrap();

    assert!(
        filtered_count < unfiltered_count,
        "expected file filter to narrow results: filtered={filtered_count} unfiltered={unfiltered_count}"
    );
    for caller in filtered_json["callers"].as_array().unwrap() {
        assert_eq!(
            caller["file"].as_str().unwrap(),
            "src/services/wallet.rs",
            "file filter leaked a caller from another file: {caller}"
        );
    }
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

// ---------------------------------------------------------------------------
// Group 6: gm_session_analyze
// ---------------------------------------------------------------------------

#[test]
fn gm_session_analyze_missing_facts_returns_error() {
    let resp = dispatch_tool("gm_session_analyze", &json!({}));
    assert_eq!(resp.get("isError"), Some(&json!(true)));
    let text = resp["content"][0]["text"].as_str().unwrap_or("");
    assert!(text.contains("facts"), "expected 'facts' in: {text}");
}

#[test]
fn gm_session_analyze_saves_facts() {
    let ctx = setup();
    let resp = ctx.dispatch(
        "gm_session_analyze",
        json!({
            "facts": [
                { "content": "RRF fusion chosen over pure semantic search", "type": "decision" },
                { "content": "Memory consolidate scans all JSONL files", "type": "convention" }
            ]
        }),
    );
    assert!(!ctx.is_error(&resp), "unexpected error: {}", ctx.text(&resp));
    let text = ctx.text(&resp);
    assert!(text.contains("2 saved"), "expected '2 saved' in: {text}");
    assert!(text.contains("0 skipped"), "expected '0 skipped' in: {text}");
}

#[test]
fn gm_session_analyze_deduplicates_on_second_call() {
    let ctx = setup();
    let facts = json!({
        "facts": [
            { "content": "Jaccard dedup threshold is 0.80 for session analyze", "type": "convention" }
        ]
    });
    // First call: should save
    ctx.dispatch("gm_session_analyze", facts.clone());
    // Second call: same fact — should be skipped
    let resp = ctx.dispatch("gm_session_analyze", facts);
    assert!(!ctx.is_error(&resp), "unexpected error: {}", ctx.text(&resp));
    let text = ctx.text(&resp);
    assert!(text.contains("1 skipped"), "expected '1 skipped' in: {text}");
}

#[test]
fn gm_session_analyze_empty_facts_array_returns_zero_saved() {
    let ctx = setup();
    let resp = ctx.dispatch("gm_session_analyze", json!({ "facts": [] }));
    assert!(!ctx.is_error(&resp), "unexpected error: {}", ctx.text(&resp));
    let text = ctx.text(&resp);
    assert!(text.contains("0 saved"), "expected '0 saved' in: {text}");
}

#[test]
fn gm_session_analyze_skips_empty_content() {
    let ctx = setup();
    let resp = ctx.dispatch(
        "gm_session_analyze",
        json!({
            "facts": [
                { "content": "", "type": "context" },
                { "content": "   ", "type": "context" },
                { "content": "valid fact for empty content test", "type": "context" }
            ]
        }),
    );
    assert!(!ctx.is_error(&resp), "unexpected error: {}", ctx.text(&resp));
    let text = ctx.text(&resp);
    assert!(text.contains("1 saved"), "expected '1 saved' in: {text}");
    assert!(text.contains("2 skipped"), "expected '2 skipped' in: {text}");
}

#[test]
fn gm_session_analyze_respects_priority_flag() {
    let ctx = setup();
    let resp = ctx.dispatch(
        "gm_session_analyze",
        json!({
            "facts": [
                { "content": "critical constraint: always use --slug not --project on CLI", "type": "bug", "priority": true }
            ]
        }),
    );
    assert!(!ctx.is_error(&resp), "unexpected error: {}", ctx.text(&resp));
    let text = ctx.text(&resp);
    assert!(text.contains("1 saved"), "expected '1 saved' in: {text}");
    // Verify it's searchable
    let search = ctx.dispatch("gm_memory_search", json!({ "query": "slug not project CLI" }));
    assert!(!ctx.is_error(&search));
    let search_text = ctx.text(&search);
    assert!(search_text.contains("results"), "expected search results: {search_text}");
}

#[test]
fn gm_session_analyze_saved_entries_appear_in_memory_list() {
    let ctx = setup();
    let unique = "session_analyze_unique_marker_abc123";
    ctx.dispatch(
        "gm_session_analyze",
        json!({ "facts": [{ "content": unique, "type": "context" }] }),
    );
    let list = ctx.dispatch("gm_memory_list", json!({}));
    assert!(!ctx.is_error(&list));
    let text = ctx.text(&list);
    assert!(text.contains(unique), "entry should appear in memory list: {text}");
}

// ---------------------------------------------------------------------------
// Group 7: Multi-project resolution — no arbitrary pick without `project`
// ---------------------------------------------------------------------------
//
// graph_helpers::resolve_project used to fall back to
// Registry::list().into_iter().next() when `project` was omitted — an
// unordered pick with zero relation to the caller's intent. It now
// delegates to resolve_project_slug (cwd-aware, no blind fallback). These
// tests register a SECOND project so any regression back to "pick
// whichever one HashMap iteration returns first" would be observable
// (both projects lack a repo_id, and the test's cwd isn't either project's
// registered path, so resolution must fail explicitly, never guess).

fn register_second_project(ctx: &TestCtx, second_slug: &str) {
    let config_path = ctx._dir.path().join(".graphmind").join("config.json");
    let mut config: Value = serde_json::from_str(&std::fs::read_to_string(&config_path).unwrap()).unwrap();
    let second_path = ctx._dir.path().join("unrelated-second-project");
    std::fs::create_dir_all(&second_path).unwrap();
    config["projects"][second_slug] = json!({
        "slug": second_slug,
        "path": second_path.to_str().unwrap(),
        "last_build": null,
        "languages": [],
        "registered": "2024-01-01T00:00:00Z",
        "auto_watch": false,
        "exclude": []
    });
    std::fs::write(&config_path, config.to_string()).unwrap();
}

#[test]
fn gm_status_without_project_errors_when_multiple_registered_and_cwd_unrelated() {
    let ctx = setup();
    register_second_project(&ctx, "second-unrelated-project");

    // No `project` arg, and this test's actual cwd (wherever `cargo test`
    // runs from) is not the registered path of either project. Must fail
    // explicitly — silently picking one of the two would be exactly the
    // bug this test guards against.
    let resp = ctx.dispatch("gm_status", json!({}));
    assert!(
        ctx.is_error(&resp),
        "gm_status with no project and >1 registered, unrelated cwd, must error, got: {}",
        ctx.text(&resp)
    );
}

#[test]
fn gm_status_with_explicit_project_still_resolves_correctly() {
    let ctx = setup();
    register_second_project(&ctx, "second-unrelated-project");

    // Explicit `project` must always win regardless of how many other
    // projects are registered.
    let resp = ctx.dispatch("gm_status", json!({ "project": ctx.slug }));
    assert!(!ctx.is_error(&resp), "unexpected error: {}", ctx.text(&resp));
    let text = ctx.text(&resp);
    assert!(
        text.contains(&ctx.slug),
        "status should mention the explicitly requested slug '{}': {text}",
        ctx.slug
    );
}

// ---------------------------------------------------------------------------
// Group 8: gm_memory_update, stale vectors, batch type checks
// ---------------------------------------------------------------------------

/// Saves `content` with gm_memory_add and returns the new entry's id.
fn add_memory(ctx: &TestCtx, content: &str) -> String {
    let resp = ctx.dispatch("gm_memory_add", json!({ "content": content, "type": "context" }));
    assert!(!ctx.is_error(&resp), "add failed: {}", ctx.text(&resp));
    let text = ctx.text(&resp);
    text.split("(id: ").nth(1).and_then(|rest| rest.split(',').next()).expect("id in response").to_string()
}

fn memory_list(ctx: &TestCtx) -> String {
    let resp = ctx.dispatch("gm_memory_list", json!({}));
    assert!(!ctx.is_error(&resp), "list failed: {}", ctx.text(&resp));
    ctx.text(&resp)
}

#[test]
fn gm_memory_update_changes_content_in_place() {
    let ctx = setup();
    let id = add_memory(&ctx, "deploys run from the main branch");
    let resp = ctx.dispatch("gm_memory_update", json!({ "id": &id[..8], "content": "deploys run from release tags" }));
    assert!(!ctx.is_error(&resp), "unexpected error: {}", ctx.text(&resp));
    assert!(ctx.text(&resp).contains(&id), "reports the full id: {}", ctx.text(&resp));
    let list = memory_list(&ctx);
    assert!(list.contains(&format!("deploys run from release tags (id: {id})")), "same id, new text: {list}");
    assert!(!list.contains("main branch"), "old text gone: {list}");
}

#[test]
fn gm_memory_update_rejects_empty_patch() {
    let ctx = setup();
    let id = add_memory(&ctx, "an entry nobody edits");
    let resp = ctx.dispatch("gm_memory_update", json!({ "id": id }));
    assert!(ctx.is_error(&resp), "an update with no fields must fail: {}", ctx.text(&resp));
    assert!(ctx.text(&resp).contains("nothing to change"), "{}", ctx.text(&resp));
}

#[test]
fn gm_memory_update_unknown_id_is_an_error() {
    let ctx = setup();
    add_memory(&ctx, "some other entry");
    let resp = ctx.dispatch("gm_memory_update", json!({ "id": "ffffffff", "content": "x" }));
    assert!(ctx.is_error(&resp), "unknown id must fail: {}", ctx.text(&resp));
    assert!(ctx.text(&resp).contains("no memory entry with that id"), "{}", ctx.text(&resp));
    assert!(!memory_list(&ctx).contains("[context] x "), "nothing written");
}

#[test]
fn gm_memory_update_rejects_unknown_type() {
    let ctx = setup();
    let id = add_memory(&ctx, "type stays context");
    let resp = ctx.dispatch("gm_memory_update", json!({ "id": id, "type": "decisions", "content": "changed" }));
    assert!(ctx.is_error(&resp), "unknown type must fail: {}", ctx.text(&resp));
    assert!(ctx.text(&resp).contains("decision, pattern, convention"), "lists the valid types: {}", ctx.text(&resp));
    let list = memory_list(&ctx);
    assert!(list.contains("[context] type stays context"), "entry unchanged: {list}");
}

#[test]
fn gm_session_analyze_bad_type_saves_nothing() {
    let ctx = setup();
    let resp = ctx.dispatch(
        "gm_session_analyze",
        json!({
            "facts": [
                { "content": "a valid fact listed before the typo", "type": "decision" },
                { "content": "a fact with a typo'd type", "type": "decisions" }
            ]
        }),
    );
    assert!(ctx.is_error(&resp), "a bad type must fail the batch: {}", ctx.text(&resp));
    assert!(ctx.text(&resp).contains("decision, pattern, convention"), "lists the valid types: {}", ctx.text(&resp));
    let list = memory_list(&ctx);
    assert!(!list.contains("valid fact listed before"), "nothing saved, not even the valid fact: {list}");
}

/// Serves an OpenAI-compatible `/embeddings` endpoint on localhost that embeds every input as
/// `[1, 0]`, so every stored vector matches every query equally. Returns the base URL.
fn fake_embedding_server() -> String {
    use std::io::{BufRead, BufReader, Read, Write};
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}/v1", listener.local_addr().unwrap());
    std::thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            let mut reader = BufReader::new(stream);
            let mut length = 0usize;
            loop {
                let mut line = String::new();
                if reader.read_line(&mut line).unwrap_or(0) == 0 || line == "\r\n" {
                    break;
                }
                if let Some((name, value)) = line.split_once(':') {
                    if name.eq_ignore_ascii_case("content-length") {
                        length = value.trim().parse().unwrap_or(0);
                    }
                }
            }
            let mut body = vec![0; length];
            if reader.read_exact(&mut body).is_err() {
                continue;
            }
            let inputs = serde_json::from_slice::<Value>(&body)
                .ok()
                .and_then(|v| v["input"].as_array().map(|a| a.len()))
                .unwrap_or(1);
            let data: Vec<Value> = (0..inputs).map(|i| json!({ "index": i, "embedding": [1.0, 0.0] })).collect();
            let payload = json!({ "data": data }).to_string();
            let mut stream = reader.into_inner();
            let _ = write!(
                stream,
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{payload}",
                payload.len()
            );
        }
    });
    url
}

/// A semantic hit whose stored text no longer matches the entry's content (the entry was edited
/// without re-indexing) is dropped, so the old wording can't rank the entry.
#[test]
fn gm_memory_search_ignores_stale_vectors() {
    let ctx = setup();
    let config_path = ctx._dir.path().join(".graphmind").join("config.json");
    let mut config: Value = serde_json::from_str(&std::fs::read_to_string(&config_path).unwrap()).unwrap();
    config["embedding"] = json!({
        "mode": "openai",
        "openai_base_url": fake_embedding_server(),
        "api_keys": { "openai": "test-key" }
    });
    std::fs::write(&config_path, config.to_string()).unwrap();

    // Both entries get a vector through gm_memory_add; then the second one's row is rewritten
    // with text the entry no longer has, as an edit made without re-indexing would leave it.
    add_memory(&ctx, "fresh entry about caching");
    let stale = add_memory(&ctx, "stale entry about retries");
    let db = ctx._dir.path().join(".graphmind").join("memory").join("memory_embeddings.db");
    let store = graphmind_embeddings::store::EmbeddingStore::open(&db).unwrap();
    assert_eq!(store.all().unwrap().len(), 2, "both entries were embedded");
    store
        .replace_symbol(&graphmind_embeddings::store::NewEmbeddingRow {
            symbol_name: stale,
            symbol_kind: "memory".into(),
            file: String::new(),
            text: "the wording before an edit".into(),
            embedding: graphmind_embeddings::store::float32_to_bytes(&[1.0, 0.0]),
        })
        .unwrap();

    // Matches no entry's words, so anything returned came from semantic search.
    let resp = ctx.dispatch("gm_memory_search", json!({ "query": "zqxv" }));
    assert!(!ctx.is_error(&resp), "unexpected error: {}", ctx.text(&resp));
    let text = ctx.text(&resp);
    assert!(text.contains("fresh entry about caching"), "a current vector still matches: {text}");
    assert!(!text.contains("stale entry about retries"), "a stale vector is ignored: {text}");
}

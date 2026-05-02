use graphmind_db::builder::{BuildOptions, GraphBuilder};
use graphmind_db::queries::GraphQueries;
use std::path::Path;

fn fixture_project() -> String {
    let manifest = env!("CARGO_MANIFEST_DIR");
    Path::new(manifest)
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("tests/fixtures/sample-project")
        .to_string_lossy()
        .to_string()
}

fn build_fixture() -> GraphBuilder {
    let tmp = tempfile::tempdir().unwrap();
    let db_path = tmp.path().join("test.db");
    let cache_dir = tmp.path().join("cache");
    std::fs::create_dir_all(&cache_dir).unwrap();

    let mut builder = GraphBuilder::new(
        db_path.to_str().unwrap(),
        cache_dir.to_str().unwrap(),
    );

    let result = builder.build(
        &fixture_project(),
        &BuildOptions { full: true, ..Default::default() },
    );

    eprintln!(
        "Build: {} files, {} symbols, {} edges in {}ms",
        result.files_processed, result.symbols_found, result.edges_created, result.duration_ms
    );

    // Leak the tempdir so files survive for queries
    std::mem::forget(tmp);

    builder
}

// ── Build stats ──────────────────────────────────────────────────

#[test]
fn build_processes_all_files() {
    let builder = build_fixture();
    let q = GraphQueries::new(builder.database());
    let stats = q.stats();

    assert!(stats.files >= 5, "expected >= 5 files, got {}", stats.files);
    assert!(stats.symbols >= 10, "expected >= 10 symbols, got {}", stats.symbols);
    assert!(stats.edges >= 5, "expected >= 5 edges, got {}", stats.edges);
}

// ── Symbol lookup ────────────────────────────────────────────────

#[test]
fn find_symbol_by_exact_name() {
    let builder = build_fixture();
    let q = GraphQueries::new(builder.database());

    let results = q.find_symbol("createWallet");
    assert!(!results.is_empty(), "createWallet not found");
    assert_eq!(results[0].kind, "Method");
    assert!(results[0].file.contains("services/wallet"));
}

#[test]
fn find_class_symbol() {
    let builder = build_fixture();
    let q = GraphQueries::new(builder.database());

    let results = q.find_symbol("WalletService");
    assert!(!results.is_empty(), "WalletService not found");
    assert_eq!(results[0].kind, "Class");
}

#[test]
fn find_interface_symbol() {
    let builder = build_fixture();
    let q = GraphQueries::new(builder.database());

    let results = q.find_symbol("Wallet");
    assert!(!results.is_empty(), "Wallet not found");
    assert!(
        results.iter().any(|r| r.kind == "Interface"),
        "expected at least one Wallet as Interface, got kinds: {:?}",
        results.iter().map(|r| r.kind.as_str()).collect::<Vec<_>>()
    );
}

// ── FTS search ───────────────────────────────────────────────────

#[test]
fn fts_search_finds_symbol() {
    let builder = build_fixture();
    let q = GraphQueries::new(builder.database());

    let results = q.search_symbols("wallet*", 10);
    let names: Vec<&str> = results.iter().map(|r| r.name.as_str()).collect();

    assert!(
        names.iter().any(|n| n.contains("allet")),
        "FTS search for 'wallet*' should find wallet-related symbols, got {:?}",
        names
    );
}

#[test]
fn fts_search_prefix_matching() {
    let builder = build_fixture();
    let q = GraphQueries::new(builder.database());

    let results = q.search_symbols("validate*", 10);
    assert!(
        !results.is_empty(),
        "prefix search 'validate*' should find validateAddress"
    );
    assert!(results.iter().any(|r| r.name == "validateAddress"));
}

// ── Callers / Callees ────────────────────────────────────────────

#[test]
fn callers_of_create_wallet() {
    let builder = build_fixture();
    let q = GraphQueries::new(builder.database());

    let callers = q.callers("createWallet");
    let caller_names: Vec<&str> = callers.iter().map(|c| c.name.as_str()).collect();

    assert!(
        caller_names.contains(&"handleCreateWallet"),
        "handleCreateWallet should be a caller of createWallet, got {:?}",
        caller_names
    );
}

#[test]
fn callees_of_create_wallet() {
    let builder = build_fixture();
    let q = GraphQueries::new(builder.database());

    let callees = q.callees("createWallet");
    let callee_names: Vec<&str> = callees.iter().map(|c| c.name.as_str()).collect();

    assert!(
        callee_names.contains(&"validateAddress"),
        "createWallet should call validateAddress, got {:?}",
        callee_names
    );
    // logger.info no longer resolves via global fallback (receiver-aware: confidence < 0.5)
    assert!(
        !callee_names.contains(&"info"),
        "createWallet should NOT resolve logger.info to unrelated 'info' symbol, got {:?}",
        callee_names
    );
}

#[test]
fn member_expression_edges_resolved() {
    let builder = build_fixture();
    let q = GraphQueries::new(builder.database());

    // The key regression test: walletService.createWallet() in routes
    // must create an edge to the method, not to "walletService.createWallet"
    let callers = q.callers("createWallet");
    assert!(
        !callers.is_empty(),
        "createWallet should have callers (member expression must be resolved)"
    );

    // There should be NO symbol with a dot in its name
    let dotted = q.find_symbol("walletService.createWallet");
    assert!(
        dotted.is_empty(),
        "should NOT have a symbol 'walletService.createWallet' — member expression not resolved"
    );
}

// ── File dependencies ────────────────────────────────────────────

#[test]
fn file_deps_from_route() {
    let builder = build_fixture();
    let q = GraphQueries::new(builder.database());

    let deps = q.file_deps("src/routes/wallet.ts");
    let dep_files: Vec<&str> = deps.iter().map(|d| d.file.as_str()).collect();

    assert!(
        dep_files.iter().any(|f| f.contains("services/wallet")),
        "routes/wallet should depend on services/wallet, got {:?}",
        dep_files
    );
}

#[test]
fn file_reverse_deps_of_service() {
    let builder = build_fixture();
    let q = GraphQueries::new(builder.database());

    let rdeps = q.file_reverse_deps("src/services/wallet.ts");
    let rdep_files: Vec<&str> = rdeps.iter().map(|d| d.file.as_str()).collect();

    assert!(
        rdep_files.iter().any(|f| f.contains("routes/wallet")),
        "services/wallet should be depended on by routes/wallet, got {:?}",
        rdep_files
    );
}

// ── Impact analysis ──────────────────────────────────────────────

#[test]
fn impact_of_validator() {
    let builder = build_fixture();
    let q = GraphQueries::new(builder.database());

    let impacted = q.impact("src/utils/validator.ts", 5);

    assert!(
        impacted.iter().any(|f| f.contains("services/wallet")),
        "changing validator should impact services/wallet, got {:?}",
        impacted
    );
}

// ── Top connected ────────────────────────────────────────────────

#[test]
fn top_connected_files() {
    let builder = build_fixture();
    let q = GraphQueries::new(builder.database());

    let top = q.top_connected(5);
    assert!(!top.is_empty(), "should have connected files");
    assert!(top[0].connections > 0, "top file should have connections");
}

// ── Content stored ───────────────────────────────────────────────

#[test]
fn symbol_content_stored() {
    let builder = build_fixture();
    let q = GraphQueries::new(builder.database());

    let results = q.find_symbol("createWallet");
    assert!(!results.is_empty());

    let content = results[0].content.as_deref().unwrap_or("");
    assert!(
        content.contains("createWallet"),
        "symbol content should contain function body"
    );
    assert!(
        content.contains("validateAddress"),
        "createWallet body should contain validateAddress call"
    );
}

// ── Symbols in file ──────────────────────────────────────────────

#[test]
fn symbols_in_file() {
    let builder = build_fixture();
    let q = GraphQueries::new(builder.database());

    let syms = q.symbols_in_file("src/utils/validator.ts");
    let names: Vec<&str> = syms.iter().map(|s| s.name.as_str()).collect();

    assert!(names.contains(&"validateAddress"));
    assert!(names.contains(&"sanitizeInput"));
    assert_eq!(names.len(), 2);
}

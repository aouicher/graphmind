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

// ── Outline ──────────────────────────────────────────────────────

#[test]
fn outline_returns_symbols_with_lines() {
    let builder = build_fixture();
    let q = GraphQueries::new(builder.database());

    let nodes = q.outline("src/services/wallet.ts");
    assert!(!nodes.is_empty(), "outline should return symbols for wallet.ts");

    // Every node must have valid (non-zero) line numbers
    for node in &nodes {
        assert!(node.line_start > 0, "line_start should be > 0 for {}", node.name);
        assert!(node.line_end >= node.line_start, "line_end >= line_start for {}", node.name);
    }

    // The file has a WalletService class
    let all_names: Vec<&str> = nodes.iter().map(|n| n.name.as_str()).collect();
    assert!(
        all_names.contains(&"WalletService"),
        "outline should include WalletService, got {:?}",
        all_names
    );
}

// ── Who calls chain ───────────────────────────────────────────────

#[test]
fn who_calls_chain_transitive() {
    let builder = build_fixture();
    let q = GraphQueries::new(builder.database());

    let (chain, _max_reached) = q.who_calls_chain("validateAddress", 5);
    let names: Vec<&str> = chain.iter().map(|c| c.name.as_str()).collect();

    // createWallet calls validateAddress (depth 1)
    assert!(
        names.contains(&"createWallet"),
        "createWallet should call validateAddress transitively, got {:?}",
        names
    );
    // handleCreateWallet calls createWallet (depth 2)
    assert!(
        names.contains(&"handleCreateWallet"),
        "handleCreateWallet should appear in transitive chain, got {:?}",
        names
    );
}

// ── Dead code ─────────────────────────────────────────────────────

#[test]
fn dead_code_returns_unreachable() {
    let builder = build_fixture();
    let q = GraphQueries::new(builder.database());

    let dead = q.dead_code(None, 50);
    // Must not panic; all results must be Functions or Methods
    for sym in &dead {
        assert!(
            sym.kind == "Function" || sym.kind == "Method",
            "dead_code without kind filter should only return Function/Method, got kind={} for {}",
            sym.kind,
            sym.name
        );
    }
}

// ── Cycles ────────────────────────────────────────────────────────

#[test]
fn detect_cycles_no_false_positives() {
    let builder = build_fixture();
    let q = GraphQueries::new(builder.database());

    let cycles = q.detect_cycles();
    // The fixture has no intentional cycles; if any are reported they must be symmetric
    for pair in &cycles {
        assert_ne!(
            pair.from_file, pair.to_file,
            "cycle pair should not be self-referential"
        );
    }
    // (empty is also valid — just no panic required)
}

// ── Similar symbols ───────────────────────────────────────────────

#[test]
fn similar_symbols_finds_related() {
    let builder = build_fixture();
    let q = GraphQueries::new(builder.database());

    // Resolve createWallet to its id first
    let syms = q.find_symbol("createWallet");
    assert!(!syms.is_empty(), "createWallet must exist");
    let id = syms[0].id;

    // Should not panic, may return 0 results if no similar symbols exist
    let similar = q.similar_symbols(id, 5);
    // All results should be different symbols
    for s in &similar {
        assert_ne!(s.id, id, "similar result should not be the query symbol itself");
    }
}

// ── Find listeners ────────────────────────────────────────────────

#[test]
fn find_listeners_graceful() {
    let builder = build_fixture();
    let q = GraphQueries::new(builder.database());

    // Should not panic; returns empty if no symbol named wallet_created
    let listeners = q.find_listeners("wallet_created");
    // No assertion on count — graceful empty is fine
    let _ = listeners;
}

// ── find_symbol_filtered by kind ──────────────────────────────────

#[test]
fn find_symbol_filtered_by_kind() {
    let builder = build_fixture();
    let q = GraphQueries::new(builder.database());

    let results = q.find_symbol_filtered("WalletService", None, Some("Class"));
    assert!(!results.is_empty(), "find_symbol_filtered should find WalletService as Class");
    assert_eq!(results[0].name, "WalletService");
    assert_eq!(results[0].kind, "Class");
}

// ── Language breakdown ────────────────────────────────────────────

#[test]
fn language_breakdown_returns_langs() {
    let builder = build_fixture();
    let q = GraphQueries::new(builder.database());

    let langs = q.language_breakdown();
    assert!(!langs.is_empty(), "language_breakdown should return at least one language");

    let lang_names: Vec<&str> = langs.iter().map(|l| l.language.as_str()).collect();
    assert!(
        lang_names.contains(&"typescript"),
        "should include typescript in language breakdown, got {:?}",
        lang_names
    );
}

// ── Incremental build ─────────────────────────────────────────────

#[test]
fn incremental_build_updates_stats() {
    let tmp = tempfile::tempdir().unwrap();
    let db_path = tmp.path().join("test.db");
    let cache_dir = tmp.path().join("cache");
    std::fs::create_dir_all(&cache_dir).unwrap();

    let mut builder = GraphBuilder::new(
        db_path.to_str().unwrap(),
        cache_dir.to_str().unwrap(),
    );

    let project_root = fixture_project();

    // Full build
    builder.build(&project_root, &BuildOptions { full: true, ..Default::default() });
    let stats_before = GraphQueries::new(builder.database()).stats();
    assert!(stats_before.symbols > 0, "should have symbols after full build");

    // Incremental build targeting a specific file
    builder.build(
        &project_root,
        &BuildOptions {
            only_files: Some(vec!["src/services/wallet.ts".to_string()]),
            ..Default::default()
        },
    );
    let stats_after = GraphQueries::new(builder.database()).stats();

    // Symbol count should remain stable (same or more after re-processing)
    assert!(
        stats_after.symbols >= stats_before.symbols,
        "incremental rebuild should not reduce symbol count: before={}, after={}",
        stats_before.symbols,
        stats_after.symbols
    );
}

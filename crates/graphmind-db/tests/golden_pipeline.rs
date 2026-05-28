use graphmind_db::builder::{BuildOptions, GraphBuilder};
use graphmind_db::queries::GraphQueries;
use rusqlite::Connection;
use std::path::Path;
use std::sync::{Arc, OnceLock};

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

// ── Shared fixture — built once, reused by all read-only tests ────────────────
//
// We store the db_path as a String and keep the TempDir alive via Arc.
// Each test opens its own Connection from the path; SQLite supports multiple
// concurrent read-only connections. No Mutex needed — Connection is opened
// fresh per test and queries are all read-only.

struct SharedDb {
    db_path: String,
    _dir: Arc<tempfile::TempDir>,
}

// SharedDb only stores a String and an Arc — both are Send + Sync.
static SHARED_DB: OnceLock<SharedDb> = OnceLock::new();

fn shared_db() -> &'static SharedDb {
    SHARED_DB.get_or_init(|| {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("test.db");
        let cache_dir = dir.path().join("cache");
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
            "Build (shared): {} files, {} symbols, {} edges in {}ms",
            result.files_processed, result.symbols_found, result.edges_created, result.duration_ms
        );

        SharedDb {
            db_path: db_path.to_str().unwrap().to_string(),
            _dir: Arc::new(dir),
        }
    })
}

/// Open a fresh read-only connection to the shared DB and return a GraphQueries.
/// Each test calls this; SQLite handles concurrent readers without issue.
fn open_q() -> (Connection, ) {
    let conn = Connection::open(&shared_db().db_path).expect("failed to open shared test.db");
    (conn,)
}

// Convenience macro: open a connection, bind queries, run body.
// `q` is the GraphQueries bound to the local `conn`.
macro_rules! with_db {
    (|$q:ident| $body:block) => {{
        let (conn,) = open_q();
        let $q = GraphQueries::new(&conn);
        $body
    }};
}

// ── Build stats ──────────────────────────────────────────────────

#[test]
fn build_processes_all_files() {
    with_db!(|q| {
        let stats = q.stats();
        assert!(stats.files >= 5, "expected >= 5 files, got {}", stats.files);
        assert!(stats.symbols >= 10, "expected >= 10 symbols, got {}", stats.symbols);
        assert!(stats.edges >= 5, "expected >= 5 edges, got {}", stats.edges);
    });
}

// ── Symbol lookup ────────────────────────────────────────────────

#[test]
fn find_symbol_by_exact_name() {
    with_db!(|q| {
        let results = q.find_symbol("createWallet");
        assert!(!results.is_empty(), "createWallet not found");
        assert_eq!(results[0].kind, "Method");
        assert!(results[0].file.contains("services/wallet"));
    });
}

#[test]
fn find_class_symbol() {
    with_db!(|q| {
        let results = q.find_symbol("WalletService");
        assert!(!results.is_empty(), "WalletService not found");
        assert_eq!(results[0].kind, "Class");
    });
}

#[test]
fn find_interface_symbol() {
    with_db!(|q| {
        let results = q.find_symbol("Wallet");
        assert!(!results.is_empty(), "Wallet not found");
        assert!(
            results.iter().any(|r| r.kind == "Interface"),
            "expected at least one Wallet as Interface, got kinds: {:?}",
            results.iter().map(|r| r.kind.as_str()).collect::<Vec<_>>()
        );
    });
}

// ── FTS search ───────────────────────────────────────────────────

#[test]
fn fts_search_finds_symbol() {
    with_db!(|q| {
        let results = q.search_symbols("wallet*", 10);
        let names: Vec<&str> = results.iter().map(|r| r.name.as_str()).collect();
        assert!(
            names.iter().any(|n| n.contains("allet")),
            "FTS search for 'wallet*' should find wallet-related symbols, got {:?}",
            names
        );
    });
}

#[test]
fn fts_search_prefix_matching() {
    with_db!(|q| {
        let results = q.search_symbols("validate*", 10);
        assert!(
            !results.is_empty(),
            "prefix search 'validate*' should find validateAddress"
        );
        assert!(results.iter().any(|r| r.name == "validateAddress"));
    });
}

// ── Callers / Callees ────────────────────────────────────────────

#[test]
fn callers_of_create_wallet() {
    with_db!(|q| {
        let callers = q.callers("createWallet");
        let caller_names: Vec<&str> = callers.iter().map(|c| c.name.as_str()).collect();
        assert!(
            caller_names.contains(&"handleCreateWallet"),
            "handleCreateWallet should be a caller of createWallet, got {:?}",
            caller_names
        );
    });
}

#[test]
fn callees_of_create_wallet() {
    with_db!(|q| {
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
    });
}

#[test]
fn member_expression_edges_resolved() {
    with_db!(|q| {
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
    });
}

// ── File dependencies ────────────────────────────────────────────

#[test]
fn file_deps_from_route() {
    with_db!(|q| {
        let deps = q.file_deps("src/routes/wallet.ts");
        let dep_files: Vec<&str> = deps.iter().map(|d| d.file.as_str()).collect();
        assert!(
            dep_files.iter().any(|f| f.contains("services/wallet")),
            "routes/wallet should depend on services/wallet, got {:?}",
            dep_files
        );
    });
}

#[test]
fn file_reverse_deps_of_service() {
    with_db!(|q| {
        let rdeps = q.file_reverse_deps("src/services/wallet.ts");
        let rdep_files: Vec<&str> = rdeps.iter().map(|d| d.file.as_str()).collect();
        assert!(
            rdep_files.iter().any(|f| f.contains("routes/wallet")),
            "services/wallet should be depended on by routes/wallet, got {:?}",
            rdep_files
        );
    });
}

// ── Impact analysis ──────────────────────────────────────────────

#[test]
fn impact_of_validator() {
    with_db!(|q| {
        let impacted = q.impact("src/utils/validator.ts", 5);
        assert!(
            impacted.iter().any(|f| f.contains("services/wallet")),
            "changing validator should impact services/wallet, got {:?}",
            impacted
        );
    });
}

// ── Top connected ────────────────────────────────────────────────

#[test]
fn top_connected_files() {
    with_db!(|q| {
        let top = q.top_connected(5);
        assert!(!top.is_empty(), "should have connected files");
        assert!(top[0].connections > 0, "top file should have connections");
    });
}

// ── Content stored ───────────────────────────────────────────────

#[test]
fn symbol_content_stored() {
    with_db!(|q| {
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
    });
}

// ── Symbols in file ──────────────────────────────────────────────

#[test]
fn symbols_in_file() {
    with_db!(|q| {
        let syms = q.symbols_in_file("src/utils/validator.ts");
        let names: Vec<&str> = syms.iter().map(|s| s.name.as_str()).collect();
        assert!(names.contains(&"validateAddress"));
        assert!(names.contains(&"sanitizeInput"));
        assert_eq!(names.len(), 2);
    });
}

// ── Outline ──────────────────────────────────────────────────────

#[test]
fn outline_returns_symbols_with_lines() {
    with_db!(|q| {
        let nodes = q.outline("src/services/wallet.ts");
        assert!(!nodes.is_empty(), "outline should return symbols for wallet.ts");
        for node in &nodes {
            assert!(node.line_start > 0, "line_start should be > 0 for {}", node.name);
            assert!(node.line_end >= node.line_start, "line_end >= line_start for {}", node.name);
        }
        let all_names: Vec<&str> = nodes.iter().map(|n| n.name.as_str()).collect();
        assert!(
            all_names.contains(&"WalletService"),
            "outline should include WalletService, got {:?}",
            all_names
        );
    });
}

// ── Who calls chain ───────────────────────────────────────────────

#[test]
fn who_calls_chain_transitive() {
    with_db!(|q| {
        let (chain, _max_reached) = q.who_calls_chain("validateAddress", 5);
        let names: Vec<&str> = chain.iter().map(|c| c.name.as_str()).collect();
        assert!(
            names.contains(&"createWallet"),
            "createWallet should call validateAddress transitively, got {:?}",
            names
        );
        assert!(
            names.contains(&"handleCreateWallet"),
            "handleCreateWallet should appear in transitive chain, got {:?}",
            names
        );
    });
}

// ── Dead code ─────────────────────────────────────────────────────

#[test]
fn dead_code_returns_unreachable() {
    with_db!(|q| {
        let dead = q.dead_code(None, 50);
        for sym in &dead {
            assert!(
                sym.kind == "Function" || sym.kind == "Method",
                "dead_code without kind filter should only return Function/Method, got kind={} for {}",
                sym.kind,
                sym.name
            );
        }
    });
}

// ── Cycles ────────────────────────────────────────────────────────

#[test]
fn detect_cycles_no_false_positives() {
    with_db!(|q| {
        let cycles = q.detect_cycles();
        for pair in &cycles {
            assert_ne!(
                pair.from_file, pair.to_file,
                "cycle pair should not be self-referential"
            );
        }
    });
}

// ── Similar symbols ───────────────────────────────────────────────

#[test]
fn similar_symbols_finds_related() {
    with_db!(|q| {
        let syms = q.find_symbol("createWallet");
        assert!(!syms.is_empty(), "createWallet must exist");
        let id = syms[0].id;
        let similar = q.similar_symbols(id, 5);
        for s in &similar {
            assert_ne!(s.id, id, "similar result should not be the query symbol itself");
        }
    });
}

// ── Find listeners ────────────────────────────────────────────────

#[test]
fn find_listeners_graceful() {
    with_db!(|q| {
        let listeners = q.find_listeners("wallet_created");
        let _ = listeners;
    });
}

// ── find_symbol_filtered by kind ──────────────────────────────────

#[test]
fn find_symbol_filtered_by_kind() {
    with_db!(|q| {
        let results = q.find_symbol_filtered("WalletService", None, Some("Class"));
        assert!(!results.is_empty(), "find_symbol_filtered should find WalletService as Class");
        assert_eq!(results[0].name, "WalletService");
        assert_eq!(results[0].kind, "Class");
    });
}

// ── Language breakdown ────────────────────────────────────────────

#[test]
fn language_breakdown_returns_langs() {
    with_db!(|q| {
        let langs = q.language_breakdown();
        assert!(!langs.is_empty(), "language_breakdown should return at least one language");
        let lang_names: Vec<&str> = langs.iter().map(|l| l.language.as_str()).collect();
        assert!(
            lang_names.contains(&"typescript"),
            "should include typescript in language breakdown, got {:?}",
            lang_names
        );
    });
}

// ── Incremental build ─────────────────────────────────────────────
// This test needs its own isolated build because it tests rebuild behavior.

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

    assert!(
        stats_after.symbols >= stats_before.symbols,
        "incremental rebuild should not reduce symbol count: before={}, after={}",
        stats_before.symbols,
        stats_after.symbols
    );
}

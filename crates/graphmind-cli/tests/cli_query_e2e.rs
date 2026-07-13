mod common;
use common::QueryTestEnv;

// All tests here reuse a single pre-built graph (copied per test, not rebuilt).
// This keeps the suite fast even with --test-threads=1.

#[test]
fn query_finds_symbol() {
    let env = QueryTestEnv::new();
    let out = env.run_ok(&["query", "WalletService", "--in", &env.slug]);
    assert!(out.contains("WalletService"), "query should find WalletService, got: {out}");
}

#[test]
fn fn_shows_source() {
    let env = QueryTestEnv::new();
    let out = env.run_ok(&["fn", "WalletService", "--in", &env.slug]);
    assert!(out.contains("WalletService"), "fn should show WalletService, got: {out}");
}

#[test]
fn fn_with_file_filter() {
    let env = QueryTestEnv::new();
    let out = env.run(&["fn", "WalletService", "--in", &env.slug, "--file", "src/services/wallet.ts"]);
    assert!(out.status.success(), "fn with file filter should succeed, stderr: {}", String::from_utf8_lossy(&out.stderr));
}

#[test]
fn deps_returns_output() {
    let env = QueryTestEnv::new();
    let out = env.run_ok(&["deps", "src/services/wallet.ts", "--in", &env.slug]);
    assert!(!out.trim().is_empty(), "deps should produce some output");
}

#[test]
fn impact_returns_output() {
    let env = QueryTestEnv::new();
    let out = env.run(&["impact", "src/utils/validator.ts", "--in", &env.slug]);
    assert!(out.status.success(), "impact should succeed, stderr: {}", String::from_utf8_lossy(&out.stderr));
}

#[test]
fn map_returns_stats() {
    let env = QueryTestEnv::new();
    let out = env.run_ok(&["map", &env.slug]);
    assert!(
        out.contains("symbol") || out.contains("file") || out.contains("edge"),
        "map should mention symbols/files/edges, got: {out}"
    );
}

#[test]
fn search_finds_results() {
    let env = QueryTestEnv::new();
    let out = env.run_ok(&["search", "wallet", "--in", &env.slug]);
    assert!(out.to_lowercase().contains("wallet"), "search should find wallet results, got: {out}");
}

#[test]
fn search_no_results_graceful() {
    let env = QueryTestEnv::new();
    let out = env.run(&["search", "xyznonexistentquery999", "--in", &env.slug]);
    assert!(out.status.success(), "search no results should exit 0, stderr: {}", String::from_utf8_lossy(&out.stderr));
}

#[test]
fn outline_returns_symbols() {
    let env = QueryTestEnv::new();
    let out = env.run_ok(&["outline", "src/services/wallet.ts", "--in", &env.slug]);
    assert!(out.contains("WalletService"), "outline should contain WalletService, got: {out}");
}

#[test]
fn who_calls_returns_chain() {
    let env = QueryTestEnv::new();
    let out = env.run(&["who-calls", "validateAddress", "--in", &env.slug]);
    assert!(out.status.success(), "who-calls should succeed, stderr: {}", String::from_utf8_lossy(&out.stderr));
    let stdout = String::from_utf8_lossy(&out.stdout).to_string();
    assert!(
        stdout.contains("validateAddress") || stdout.contains("createWallet"),
        "who-calls chain should contain callers, got: {stdout}"
    );
}

#[test]
fn dead_code_runs() {
    let env = QueryTestEnv::new();
    let out = env.run(&["dead-code", "--in", &env.slug]);
    assert!(out.status.success(), "dead-code should exit 0, stderr: {}", String::from_utf8_lossy(&out.stderr));
}

#[test]
fn fn_impact_runs() {
    let env = QueryTestEnv::new();
    let out = env.run(&["fn-impact", "validateAddress", "--in", &env.slug]);
    assert!(out.status.success(), "fn-impact should exit 0, stderr: {}", String::from_utf8_lossy(&out.stderr));
}

#[test]
fn fn_impact_with_file_filter_narrows_ambiguous_symbol() {
    // "fetch_balance" is defined identically in wallet.rs and wallet.rb in the
    // fixture. Without --file, impact is unioned across every match; with
    // --file, it should be scoped to the file-specific definition only.
    let env = QueryTestEnv::new();
    let unfiltered = env.run_ok(&["fn-impact", "fetch_balance", "--in", &env.slug]);
    let filtered = env.run_ok(&[
        "fn-impact",
        "fetch_balance",
        "--in", &env.slug,
        "--file", "src/services/wallet.rs",
    ]);
    assert!(filtered.contains("files impacted"), "expected impact output, got: {filtered}");

    let count = |s: &str| -> usize {
        s.lines()
            .find(|l| l.contains("files impacted"))
            .and_then(|l| l.split_whitespace().nth(1))
            .and_then(|n| n.parse().ok())
            .unwrap_or(0)
    };
    assert!(
        count(&filtered) <= count(&unfiltered),
        "expected --file to narrow or match impact count: filtered={filtered} unfiltered={unfiltered}"
    );
}

#[test]
fn cycles_runs() {
    let env = QueryTestEnv::new();
    let out = env.run(&["cycles", &env.slug]);
    assert!(out.status.success(), "cycles should exit 0, stderr: {}", String::from_utf8_lossy(&out.stderr));
}

mod common;
use common::{CliTestEnv, QueryTestEnv};

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

/// A project with indexed `src/lib.rs` and `src/link.rs` and an unindexed `.env`, next to a
/// file outside it, registered and built as `file-guard`. Returns the temp dir holding both.
fn file_guard_project(env: &CliTestEnv) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let project = dir.path().join("proj");
    std::fs::create_dir_all(project.join("src")).unwrap();
    std::fs::write(project.join("src/lib.rs"), "pub fn inside_fn() {}\n").unwrap();
    std::fs::write(project.join("src/link.rs"), "pub fn link_fn() {}\n").unwrap();
    std::fs::write(project.join(".env"), "ENV_SECRET=1\n").unwrap();
    std::fs::write(dir.path().join("outside.rs"), "pub fn outside_secret() {}\n").unwrap();
    env.run_ok(&["register", project.to_str().unwrap(), "--slug", "file-guard"]);
    env.run_ok(&["build", "file-guard"]);
    dir
}

/// Runs `graphmind file <path>` and asserts it is refused: non-zero exit, `secret` absent
/// from the output, `reason` in the error.
fn assert_file_refused(env: &CliTestEnv, path: &str, secret: &str, reason: &str) {
    let out = env.run(&["file", path, "--in", "file-guard"]);
    let (stdout, stderr) = (String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
    assert!(!out.status.success(), "{path} should be refused, stdout: {stdout}");
    assert!(!stdout.contains(secret) && !stderr.contains(secret), "{path} leaked content: {stdout}");
    assert!(stderr.contains(reason), "{path}: expected \"{reason}\" in stderr: {stderr}");
}

#[test]
fn file_serves_indexed_file_with_and_without_dot_slash() {
    let env = CliTestEnv::new();
    let _dir = file_guard_project(&env);
    for path in ["src/lib.rs", "./src/lib.rs"] {
        let out = env.run_ok(&["file", path, "--in", "file-guard"]);
        assert!(out.contains("inside_fn"), "{path} should be served, got: {out}");
    }
}

#[test]
fn file_refuses_absolute_path() {
    let env = CliTestEnv::new();
    let dir = file_guard_project(&env);
    let outside = dir.path().join("outside.rs");
    assert_file_refused(&env, outside.to_str().unwrap(), "outside_secret", "not an indexed file of this project");
}

#[test]
fn file_refuses_parent_paths() {
    let env = CliTestEnv::new();
    let _dir = file_guard_project(&env);
    for path in ["../outside.rs", "src/../../outside.rs"] {
        assert_file_refused(&env, path, "outside_secret", "not an indexed file of this project");
    }
}

#[test]
fn file_refuses_unindexed_file_inside_the_project() {
    let env = CliTestEnv::new();
    let _dir = file_guard_project(&env);
    assert_file_refused(&env, ".env", "ENV_SECRET", "not an indexed file of this project");
}

#[cfg(unix)]
#[test]
fn file_refuses_indexed_path_that_is_now_a_symlink_leading_out() {
    let env = CliTestEnv::new();
    let dir = file_guard_project(&env);
    let link = dir.path().join("proj/src/link.rs");
    std::fs::remove_file(&link).unwrap();
    std::os::unix::fs::symlink(dir.path().join("outside.rs"), &link).unwrap();
    assert_file_refused(&env, "src/link.rs", "outside_secret", "resolves outside the project directory");
}

#[cfg(unix)]
#[test]
fn file_refuses_links_to_ignored_files_and_serves_links_to_indexed_ones() {
    use std::os::unix::fs::symlink;
    let env = CliTestEnv::new();
    let dir = tempfile::tempdir().unwrap();
    let project = dir.path().join("proj");
    std::fs::create_dir_all(project.join("src")).unwrap();
    std::fs::create_dir_all(project.join("secrets")).unwrap();
    std::fs::write(project.join(".gitignore"), "secrets/\n").unwrap();
    std::fs::write(project.join(".env"), "ENV_SECRET=1\n").unwrap();
    std::fs::write(project.join("secrets/prod.toml"), "api_key = \"PROD_SECRET\"\n").unwrap();
    std::fs::write(project.join("src/lib.rs"), "pub fn inside_fn() {}\n").unwrap();
    symlink(".env", project.join("notes.md")).unwrap();
    symlink("secrets/prod.toml", project.join("conf.toml")).unwrap();
    symlink("lib.rs", project.join("src/alias.rs")).unwrap();
    env.run_ok(&["register", project.to_str().unwrap(), "--slug", "file-guard"]);
    env.run_ok(&["build", "file-guard"]);

    assert_file_refused(&env, "notes.md", "ENV_SECRET", "not an indexed file of this project");
    assert_file_refused(&env, "conf.toml", "PROD_SECRET", "not an indexed file of this project");
    let out = env.run_ok(&["file", "src/alias.rs", "--in", "file-guard"]);
    assert!(out.contains("inside_fn"), "a link to an indexed file is served, got: {out}");
}

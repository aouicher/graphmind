mod common;
use common::CliTestEnv;

// build_creates_graph_db, build_full_flag, and build_all_flag are merged into a single
// test to share the register+build cost. They test orthogonal flags on the same slug.
#[test]
fn build_basic_flags() {
    let env = CliTestEnv::new();
    let fixture = env.fixture_path();

    // 1. Register once
    env.run_ok(&["register", fixture.to_str().unwrap(), "--slug", "build-test"]);

    // 2. Plain build — verifies graph.db is created
    env.run_ok(&["build", "build-test"]);
    let graph_db = env
        .home
        .path()
        .join(".graphmind")
        .join("graphs")
        .join("build-test")
        .join("graph.db");
    assert!(graph_db.exists(), "graph.db should exist after build, path: {}", graph_db.display());

    // 3. --full flag
    env.run_ok(&["build", "build-test", "--full"]);

    // 4. --all flag (builds every registered project)
    env.run_ok(&["build", "--all"]);
}

#[test]
fn build_reset_flag() {
    let env = CliTestEnv::new();
    let fixture = env.fixture_path();
    env.run_ok(&["register", fixture.to_str().unwrap(), "--slug", "build-reset"]);
    env.run_ok(&["build", "build-reset"]);
    env.run_ok(&["build", "build-reset", "--reset"]);
}

#[test]
fn build_unknown_slug_error() {
    let env = CliTestEnv::new();
    let out = env.run(&["build", "nonexistent-slug-xyz"]);
    // Should fail: either non-zero exit or error output
    assert!(
        !out.status.success()
            || String::from_utf8_lossy(&out.stderr).contains("not found")
            || String::from_utf8_lossy(&out.stdout).contains("not found"),
        "building unknown slug should fail or warn"
    );
}

/// `build --all` builds every project even when one fails, then exits non-zero and names the
/// projects that failed, so a script or hook can tell.
#[test]
fn build_all_exits_non_zero_when_a_project_fails() {
    let env = CliTestEnv::new();
    let fixture = env.fixture_path();
    env.run_ok(&["register", fixture.to_str().unwrap(), "--slug", "all-ok"]);
    let gone = env.home.path().join("gone-project");
    std::fs::create_dir_all(&gone).unwrap();
    std::fs::write(gone.join("lib.rs"), "pub fn f() {}\n").unwrap();
    env.run_ok(&["register", gone.to_str().unwrap(), "--slug", "all-gone"]);
    std::fs::remove_dir_all(&gone).unwrap();

    let out = env.run(&["build", "--all"]);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(!out.status.success(), "a failed project must fail the run\nstderr: {stderr}");
    assert!(stderr.contains("1 of 2 builds failed: all-gone"), "{stderr}");
    let graph_db = env.home.path().join(".graphmind/graphs/all-ok/graph.db");
    assert!(graph_db.exists(), "the other project is still built");

    // With every project buildable again, the run succeeds.
    env.run_ok(&["unregister", "all-gone"]);
    env.run_ok(&["build", "--all"]);
}

mod common;
use common::CliTestEnv;

#[test]
fn build_creates_graph_db() {
    let env = CliTestEnv::new();
    let fixture = env.fixture_path();
    env.run_ok(&["register", fixture.to_str().unwrap(), "--slug", "build-test"]);
    env.run_ok(&["build", "build-test"]);

    let graph_db = env
        .home
        .path()
        .join(".graphmind")
        .join("graphs")
        .join("build-test")
        .join("graph.db");
    assert!(graph_db.exists(), "graph.db should exist after build, path: {}", graph_db.display());
}

#[test]
fn build_full_flag() {
    let env = CliTestEnv::new();
    let fixture = env.fixture_path();
    env.run_ok(&["register", fixture.to_str().unwrap(), "--slug", "build-full"]);
    env.run_ok(&["build", "build-full", "--full"]);
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

#[test]
fn build_all_flag() {
    let env = CliTestEnv::new();
    let fixture = env.fixture_path();
    env.run_ok(&["register", fixture.to_str().unwrap(), "--slug", "build-all-test"]);
    env.run_ok(&["build", "--all"]);
}

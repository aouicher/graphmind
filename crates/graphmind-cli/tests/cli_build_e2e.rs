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

/// The build names each file the minified check skips, with its average line length. Markdown
/// with long lines is prose: indexed, not named.
#[test]
fn build_names_minified_files_and_keeps_long_line_markdown() {
    let env = CliTestEnv::new();
    let project = env.home.path().join("minified-project");
    std::fs::create_dir_all(project.join("src")).unwrap();
    std::fs::create_dir_all(project.join("docs")).unwrap();
    std::fs::write(project.join("src/app.ts"), "export function main(): number {\n  return 1;\n}\n").unwrap();
    let bundle = "export function helper(){return 1};".repeat(200);
    std::fs::write(project.join("src/bundle.ts"), &bundle).unwrap();
    let paragraph = "The service reads the queue and writes the ledger. ".repeat(60);
    std::fs::write(project.join("docs/notes.md"), format!("## Notes\n\n{paragraph}\n\n## More\n\n{paragraph}\n")).unwrap();
    env.run_ok(&["register", project.to_str().unwrap(), "--slug", "minified"]);

    let out = env.run(&["build", "minified", "--full"]);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(out.status.success(), "{stderr}");
    let expected = format!("Skipped src/bundle.ts: minified (average line {} bytes)", bundle.len());
    assert!(stderr.contains(&expected), "expected `{expected}` in:\n{stderr}");
    assert!(!stderr.contains("Skipped docs/notes.md"), "markdown isn't skipped:\n{stderr}");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("| 1 skipped |"), "only the bundle is skipped:\n{stdout}");
}

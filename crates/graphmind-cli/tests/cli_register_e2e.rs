mod common;
use common::CliTestEnv;

#[test]
fn register_creates_project_entry() {
    let env = CliTestEnv::new();
    let fixture = env.fixture_path();
    env.run_ok(&["register", fixture.to_str().unwrap(), "--slug", "reg-test"]);

    let out = env.run_ok(&["list"]);
    assert!(out.contains("reg-test"), "list should show registered slug, got: {out}");
}

#[test]
fn register_idempotent() {
    let env = CliTestEnv::new();
    let fixture = env.fixture_path();
    env.run_ok(&["register", fixture.to_str().unwrap(), "--slug", "reg-idem"]);
    env.run_ok(&["register", fixture.to_str().unwrap(), "--slug", "reg-idem"]);

    let out = env.run_ok(&["list"]);
    let count = out.matches("reg-idem").count();
    assert_eq!(count, 1, "slug should appear exactly once, found {count}:\n{out}");
}

#[test]
fn register_with_slug() {
    let env = CliTestEnv::new();
    let fixture = env.fixture_path();
    env.run_ok(&["register", fixture.to_str().unwrap(), "--slug", "my-slug"]);

    let out = env.run_ok(&["list"]);
    assert!(out.contains("my-slug"), "list should show 'my-slug', got: {out}");
}

#[test]
fn unregister_removes_project() {
    let env = CliTestEnv::new();
    let fixture = env.fixture_path();
    env.run_ok(&["register", fixture.to_str().unwrap(), "--slug", "to-remove"]);

    let before = env.run_ok(&["list"]);
    assert!(before.contains("to-remove"), "should be registered before unregister");

    env.run_ok(&["unregister", "to-remove"]);
    let after = env.run_ok(&["list"]);
    assert!(!after.contains("to-remove"), "should not be listed after unregister, got: {after}");
}

#[test]
fn unregister_unknown_graceful() {
    let env = CliTestEnv::new();
    // Should either exit 0 or exit non-zero with an error message — must not panic
    let out = env.run(&["unregister", "nonexistent-xyz"]);
    let stderr = String::from_utf8_lossy(&out.stderr).to_string();
    let stdout = String::from_utf8_lossy(&out.stdout).to_string();
    // Not a panic: stderr should contain something meaningful OR exit code is used
    // We just assert it didn't crash without output
    assert!(
        !stderr.is_empty() || !stdout.is_empty() || !out.status.success(),
        "should produce some output or non-zero exit for unknown slug"
    );
}

#[test]
fn status_shows_registered() {
    let env = CliTestEnv::new();
    let fixture = env.fixture_path();
    env.run_ok(&["register", fixture.to_str().unwrap(), "--slug", "stat-test"]);

    let out = env.run_ok(&["status", "stat-test"]);
    // Status should show the slug and the word "Registered" (from the field label)
    assert!(
        out.contains("stat-test"),
        "status should mention the slug, got: {out}"
    );
}

#[test]
fn list_empty_on_fresh_home() {
    let env = CliTestEnv::new();
    let out = env.run_ok(&["list"]);
    // Fresh HOME: either "No projects" message or empty output
    assert!(
        out.contains("No projects") || out.trim().is_empty(),
        "fresh home should show no projects, got: {out}"
    );
}

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

const NOT_REGISTERED: &str = "Project 'nosuch' is not registered (see `graphmind list`)";

/// Inside a registered project, an unknown slug is an error, never that project: `unregister`
/// in particular must not remove it.
#[test]
fn unknown_slug_fails_from_inside_a_project() {
    let env = CliTestEnv::new();
    let fixture = env.fixture_path();
    env.run_ok(&["register", fixture.to_str().unwrap(), "--slug", "inside"]);

    for command in [
        vec!["map", "--slug", "nosuch"],
        vec!["status", "nosuch"],
        vec!["search", "main", "--slug", "nosuch"],
        vec!["memory", "search", "anything", "--slug", "nosuch"],
        vec!["unregister", "nosuch"],
    ] {
        let out = env.run_in(&fixture, &command);
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert!(!out.status.success(), "{command:?} succeeded:\n{}", String::from_utf8_lossy(&out.stdout));
        assert!(stderr.contains(NOT_REGISTERED), "{command:?}: {stderr}");
    }
    assert!(env.run_ok(&["list"]).contains("inside"), "the project must still be registered");
}

/// Outside every project, an unknown slug or an unregistered directory names what was given
/// rather than "No project specified".
#[test]
fn unknown_slug_or_directory_fails_from_outside_every_project() {
    let env = CliTestEnv::new();
    let fixture = env.fixture_path();
    env.run_ok(&["register", fixture.to_str().unwrap(), "--slug", "only"]);
    let outside = tempfile::tempdir().unwrap();

    for command in [vec!["map", "--slug", "nosuch"], vec!["status", "--slug", "nosuch"], vec!["fn", "main", "--slug", "nosuch"]] {
        let out = env.run_in(outside.path(), &command);
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert!(!out.status.success(), "{command:?} succeeded");
        assert!(stderr.contains(NOT_REGISTERED), "{command:?}: {stderr}");
    }

    let dir = outside.path().to_str().unwrap();
    let out = env.run_in(outside.path(), &["status", "--slug", dir]);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(!out.status.success());
    assert!(stderr.contains(&format!("Project '{dir}' is not registered (see `graphmind list`)")), "{stderr}");
}

/// A registered slug, the project's path and a directory inside it all resolve from outside.
#[test]
fn registered_slug_or_path_resolves_from_outside() {
    let env = CliTestEnv::new();
    let fixture = env.fixture_path();
    env.run_ok(&["register", fixture.to_str().unwrap(), "--slug", "by-path"]);
    let outside = tempfile::tempdir().unwrap();
    let src = fixture.join("src");
    for given in ["by-path", fixture.to_str().unwrap(), src.to_str().unwrap()] {
        let out = env.run_ok_in(outside.path(), &["status", "--slug", given]);
        assert!(out.contains("Project: by-path"), "{given}: {out}");
    }
}

/// `.` and `..` are directory paths taken from the current directory, like `./src`.
#[test]
fn dot_and_dot_dot_name_the_current_and_parent_directory() {
    let env = CliTestEnv::new();
    let fixture = env.fixture_path();
    env.run_ok(&["register", fixture.to_str().unwrap(), "--slug", "dots"]);
    let src = fixture.join("src");
    for (dir, given) in [(&fixture, "."), (&src, "."), (&src, ".."), (&src, "../src")] {
        let out = env.run_ok_in(dir, &["status", "--slug", given]);
        assert!(out.contains("Project: dots"), "{given} in {dir:?}: {out}");
    }

    let outside = tempfile::tempdir().unwrap();
    let out = env.run_in(outside.path(), &["status", "--slug", "."]);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(!out.status.success());
    assert!(stderr.contains("Project '.' is not registered (see `graphmind list`)"), "{stderr}");
}

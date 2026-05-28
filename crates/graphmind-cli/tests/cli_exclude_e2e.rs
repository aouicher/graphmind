mod common;
use common::CliTestEnv;

#[test]
fn exclude_add_and_list() {
    let env = CliTestEnv::new();
    let fixture = env.fixture_path();
    env.run_ok(&["register", fixture.to_str().unwrap(), "--slug", "excl-test"]);

    env.run_ok(&["exclude", "add", "*.log", "--in", "excl-test"]);

    let out = env.run_ok(&["exclude", "list", "--in", "excl-test"]);
    assert!(
        out.contains("*.log"),
        "exclude list should contain '*.log', got: {out}"
    );
}

#[test]
fn exclude_remove() {
    let env = CliTestEnv::new();
    let fixture = env.fixture_path();
    env.run_ok(&["register", fixture.to_str().unwrap(), "--slug", "excl-rm"]);

    env.run_ok(&["exclude", "add", "*.log", "--in", "excl-rm"]);
    env.run_ok(&["exclude", "remove", "*.log", "--in", "excl-rm"]);

    let out = env.run_ok(&["exclude", "list", "--in", "excl-rm"]);
    assert!(
        !out.contains("*.log"),
        "*.log should be removed from exclude list, got: {out}"
    );
}

#[test]
fn exclude_global_flag() {
    let env = CliTestEnv::new();
    env.run_ok(&["exclude", "add", "*.tmp", "--global"]);

    let out = env.run_ok(&["exclude", "list", "--global"]);
    assert!(
        out.contains("*.tmp"),
        "global exclude list should contain '*.tmp', got: {out}"
    );
}

mod common;
use common::CliTestEnv;

#[test]
fn session_start_creates_entry() {
    let env = CliTestEnv::new();
    let fixture = env.fixture_path();
    env.run_ok(&["register", fixture.to_str().unwrap(), "--slug", "sess-test"]);

    let out = env.run(&["session", "start", "sess-test"]);
    assert!(
        out.status.success(),
        "session start should exit 0, stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout).to_string();
    assert!(
        stdout.contains("sess-test") || stdout.contains("Session"),
        "session start should confirm session, got: {stdout}"
    );
}

#[test]
fn session_save_appends() {
    let env = CliTestEnv::new();
    let fixture = env.fixture_path();
    env.run_ok(&["register", fixture.to_str().unwrap(), "--slug", "sess-save"]);

    env.run_ok(&["session", "start", "sess-save"]);

    let out = env.run(&["session", "save", "did some work", "--in", "sess-save"]);
    assert!(
        out.status.success(),
        "session save should exit 0, stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout).to_string();
    assert!(
        stdout.contains("sess-save") || stdout.contains("Session"),
        "session save should confirm, got: {stdout}"
    );
}

#[test]
fn session_history_runs() {
    let env = CliTestEnv::new();
    let fixture = env.fixture_path();
    env.run_ok(&["register", fixture.to_str().unwrap(), "--slug", "sess-hist"]);
    env.run_ok(&["session", "start", "sess-hist"]);

    let out = env.run(&["session", "history", "sess-hist"]);
    assert!(
        out.status.success(),
        "session history should exit 0, stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

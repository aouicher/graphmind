mod common;
use common::CliTestEnv;

#[test]
fn memory_persists_across_invocations() {
    let env = CliTestEnv::new();

    // First invocation: add a memory
    env.run_ok(&[
        "memory", "add", "persisted across invocations", "--global", "--type", "context",
    ]);

    // Second invocation: search for it
    let out = env.run(&["memory", "search", "persisted"]);
    let stdout = String::from_utf8_lossy(&out.stdout).to_string();
    assert!(
        out.status.success(),
        "memory search should exit 0, stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        stdout.contains("persisted across invocations"),
        "memory should persist across invocations, got: {stdout}"
    );
}

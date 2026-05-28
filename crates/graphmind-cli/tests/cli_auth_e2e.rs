mod common;
use common::CliTestEnv;

#[test]
fn auth_status_no_key() {
    let env = CliTestEnv::new();
    // With no license key, auth status should succeed and show free/no license state
    let out = env.run(&["auth", "status"]);
    assert!(
        out.status.success(),
        "auth status should exit 0 with no key, stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout).to_string();
    // Should mention free or some license state
    assert!(
        !stdout.trim().is_empty(),
        "auth status should produce output, got nothing"
    );
}

#[test]
fn auth_logout_no_key() {
    let env = CliTestEnv::new();
    // Logout with no key should be graceful
    let out = env.run(&["auth", "logout"]);
    assert!(
        out.status.success(),
        "auth logout with no key should exit 0, stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn auth_login_invalid_key_format() {
    let env = CliTestEnv::new();
    // Key doesn't have the right prefix — should fail with exit 1 before hitting network
    let out = env.run(&["auth", "login", "--key", "invalid_key_xyz_not_real"]);
    assert!(
        !out.status.success(),
        "auth login with malformed key should fail"
    );
    let stderr = String::from_utf8_lossy(&out.stderr).to_string();
    assert!(
        stderr.contains("invalid") || stderr.contains("Error"),
        "error message should mention invalid key, got: {stderr}"
    );
}

/// This test requires a real license server — ignored in offline/CI environments.
#[test]
#[ignore]
fn auth_login_invalid_key_server() {
    let env = CliTestEnv::new();
    // gm_live_ prefix but invalid key — requires network
    let out = env.run(&["auth", "login", "--key", "gm_live_invalid_key_xyz"]);
    assert!(
        !out.status.success(),
        "auth login with invalid gm_live_ key should fail"
    );
}

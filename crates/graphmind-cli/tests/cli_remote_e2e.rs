mod common;
use common::CliTestEnv;

// remote status — basic operation
#[test]
fn remote_status_exits_zero_no_key() {
    let env = CliTestEnv::new();
    let out = env.run(&["remote", "status"]);
    let stdout = String::from_utf8_lossy(&out.stdout).to_string();
    assert!(
        out.status.success(),
        "remote status should exit 0 with no key\nstdout: {stdout}\nstderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(stdout.contains("off") || stdout.contains("Remote mode"),
        "remote status should show mode: {stdout}");
}

#[test]
fn remote_status_shows_free_tier_by_default() {
    let env = CliTestEnv::new();
    let stdout = env.run_ok(&["remote", "status"]);
    assert!(
        stdout.to_lowercase().contains("free"),
        "remote status with no key should show Free tier: {stdout}"
    );
}

#[test]
fn remote_status_shows_off_mode_by_default() {
    let env = CliTestEnv::new();
    let stdout = env.run_ok(&["remote", "status"]);
    assert!(stdout.contains("off"), "default remote mode should be off: {stdout}");
}

// remote set off — always allowed
#[test]
fn remote_set_off_always_succeeds() {
    let env = CliTestEnv::new();
    let stdout = env.run_ok(&["remote", "set", "off"]);
    assert!(stdout.contains("off") || stdout.contains("OK"),
        "remote set off should succeed: {stdout}");
}

#[test]
fn remote_set_off_is_idempotent() {
    let env = CliTestEnv::new();
    env.run_ok(&["remote", "set", "off"]);
    // Second call should also succeed
    let stdout = env.run_ok(&["remote", "set", "off"]);
    assert!(stdout.contains("off") || stdout.contains("OK"),
        "second remote set off should still succeed: {stdout}");
}

// remote set embed — requires Embeddings tier
#[test]
fn remote_set_embed_fails_on_free_tier() {
    let env = CliTestEnv::new();
    // No license key → Free tier → should fail
    let out = env.run(&["remote", "set", "embed"]);
    assert!(
        !out.status.success(),
        "remote set embed should fail on Free tier"
    );
    let stderr = String::from_utf8_lossy(&out.stderr).to_string();
    assert!(
        stderr.contains("Embeddings") || stderr.contains("tier") || stderr.contains("Error"),
        "error should mention tier requirement: {stderr}"
    );
}

// remote set full — requires Pro tier
#[test]
fn remote_set_full_fails_on_free_tier() {
    let env = CliTestEnv::new();
    let out = env.run(&["remote", "set", "full"]);
    assert!(
        !out.status.success(),
        "remote set full should fail on Free tier"
    );
    let stderr = String::from_utf8_lossy(&out.stderr).to_string();
    assert!(
        stderr.contains("Pro") || stderr.contains("Team") || stderr.contains("tier") || stderr.contains("Error"),
        "error should mention Pro/Team requirement: {stderr}"
    );
}

// remote set unknown mode
#[test]
fn remote_set_unknown_mode_fails() {
    let env = CliTestEnv::new();
    let out = env.run(&["remote", "set", "invalid_mode"]);
    assert!(
        !out.status.success(),
        "remote set with unknown mode should fail"
    );
    let stderr = String::from_utf8_lossy(&out.stderr).to_string();
    assert!(
        stderr.contains("invalid_mode") || stderr.contains("Unknown") || stderr.contains("Error"),
        "error should mention the unknown mode: {stderr}"
    );
}

// remote set off after initial state — config persists
#[test]
fn remote_set_off_persists_in_status() {
    let env = CliTestEnv::new();
    env.run_ok(&["remote", "set", "off"]);
    let stdout = env.run_ok(&["remote", "status"]);
    assert!(stdout.contains("off"), "after set off, status should show off: {stdout}");
}

// remote status shows available modes
#[test]
fn remote_status_lists_available_modes() {
    let env = CliTestEnv::new();
    let stdout = env.run_ok(&["remote", "status"]);
    assert!(
        stdout.contains("embed") && stdout.contains("full"),
        "remote status should list available modes: {stdout}"
    );
}

// remote set off shows rebuild warning
#[test]
fn remote_set_off_shows_rebuild_note() {
    let env = CliTestEnv::new();
    let stdout = env.run_ok(&["remote", "set", "off"]);
    // The CLI should mention that embeddings will be rebuilt
    assert!(
        stdout.contains("build") || stdout.contains("rebuild") || stdout.contains("local"),
        "remote set off should mention local/rebuild: {stdout}"
    );
}

// remote set full mentions build step for MCP SSE
#[test]
#[ignore] // Requires valid Pro/Team license — offline test only verifies tier gating
fn remote_set_full_mentions_build_for_mcp() {
    // This would only run with a valid JWT. Gating tested in remote_set_full_fails_on_free_tier.
}

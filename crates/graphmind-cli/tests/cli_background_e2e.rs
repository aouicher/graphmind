//! What a command runs besides its own work (`Background` in main.rs): without a terminal only
//! auto-setup and the daily memory clean, no network checks; in a terminal, failed network
//! checks are cached instead of retried on every command.
#![cfg(unix)]

mod common;
use common::CliTestEnv;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

/// Setup current, so auto-setup has nothing to do and starts no background `setup`.
fn current_setup(env: &CliTestEnv) {
    let fixture = env.fixture_path();
    env.run_ok(&["register", fixture.to_str().unwrap(), "--slug", "bg-test"]);
    let path = env.home.path().join(".graphmind/config.json");
    let mut cfg: Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    cfg["setup_version"] = json!(graphmind_config::SETUP_VERSION);
    std::fs::write(&path, cfg.to_string()).unwrap();
}

/// A `curl` first on PATH that logs each call and fails, like an offline machine. Returns the
/// PATH to use and the log file.
fn offline_curl(env: &CliTestEnv) -> (String, PathBuf) {
    use std::os::unix::fs::PermissionsExt;
    let bin = env.home.path().join("fake-bin");
    std::fs::create_dir_all(&bin).unwrap();
    let log = env.home.path().join("curl-calls.log");
    let script = bin.join("curl");
    std::fs::write(&script, format!("#!/bin/sh\necho \"$*\" >> '{}'\nexit 7\n", log.display())).unwrap();
    std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
    (format!("{}:/usr/bin:/bin", bin.display()), log)
}

fn curl_calls(log: &Path) -> usize {
    std::fs::read_to_string(log).map(|s| s.lines().count()).unwrap_or(0)
}

/// `graphmind list` with notices enabled and captured output, as hooks and scripts run it.
fn run_without_terminal(env: &CliTestEnv, path: &str) -> Output {
    Command::new(&env.binary)
        .arg("list")
        .env("HOME", env.home.path())
        .env("PATH", path)
        .env("GRAPHMIND_NO_AUTO_BUILD", "1")
        .output()
        .unwrap()
}

#[test]
fn no_network_checks_without_a_terminal() {
    let env = CliTestEnv::new();
    current_setup(&env);
    let (path, log) = offline_curl(&env);

    let out = run_without_terminal(&env, &path);

    assert!(out.status.success());
    assert_eq!(curl_calls(&log), 0, "update/announcement checks ran: {:?}", std::fs::read_to_string(&log));
    assert!(out.stderr.is_empty(), "notices printed: {}", String::from_utf8_lossy(&out.stderr));
}

/// Hooks still get daily memory upkeep: inline (done when the command exits, stamp written) and
/// quiet (nothing on the stdout the hook captures).
#[test]
fn daily_memory_clean_runs_quietly_without_a_terminal() {
    let env = CliTestEnv::new();
    current_setup(&env);
    let (path, _) = offline_curl(&env);

    let out = run_without_terminal(&env, &path);

    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(env.home.path().join(".graphmind/memory-clean.stamp").exists(), "memory clean didn't finish");
    assert!(!stdout.contains("OK"), "clean summary on stdout: {stdout}");
}

/// In a terminal, an offline machine pays the fetch timeouts once per cache period, not on
/// every command.
#[test]
fn failed_fetches_are_cached() {
    let env = CliTestEnv::new();
    current_setup(&env);
    let (path, log) = offline_curl(&env);
    // `script` gives the command a pseudo-terminal, so it runs as interactive.
    let in_terminal = || {
        let binary = env.binary.to_string_lossy().to_string();
        let mut cmd = Command::new("script");
        if cfg!(target_os = "linux") {
            cmd.args(["-qec", &format!("'{binary}' list"), "/dev/null"]);
        } else {
            cmd.args(["-q", "/dev/null", &binary, "list"]);
        }
        cmd.env("HOME", env.home.path())
            .env("PATH", &path)
            .env("GRAPHMIND_NO_AUTO_BUILD", "1")
            .stdin(std::process::Stdio::null())
            .output()
    };
    let Ok(first) = in_terminal() else {
        return; // no `script` on this system
    };
    assert!(first.status.success(), "{}", String::from_utf8_lossy(&first.stdout));
    let after_first = curl_calls(&log);
    assert!(after_first > 0, "interactive run should check for updates");

    in_terminal().unwrap();
    assert_eq!(curl_calls(&log), after_first, "failed fetches retried on the next command");
}

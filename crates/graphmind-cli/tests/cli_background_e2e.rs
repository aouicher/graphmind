//! What a command runs besides its own work (`Background` in main.rs): without a terminal only
//! auto-setup, no network checks or memory clean; in a terminal, failed network checks are cached
//! instead of retried on every command.
#![cfg(unix)]

mod common;
use common::CliTestEnv;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::time::{Duration, Instant};

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
        .env_remove("CLAUDE_CONFIG_DIR")
        .env("PATH", path)
        .env_remove("GRAPHMIND_SKIP_NOTICES")
        .env("GRAPHMIND_NO_AUTO_BUILD", "1")
        .output()
        .unwrap()
}

/// `graphmind list` on a pseudo-terminal (`script`), so it runs as interactive. `None` when the
/// system has no `script`.
///
/// `script`'s stdin is a pipe that stays open until `script` has exited. What `script` does when
/// its stdin reaches EOF differs between implementations and versions (util-linux sends the
/// command an EOF character), and /dev/null is at EOF from the start. Kept open, `script` only
/// stops when the command does.
fn run_in_terminal(env: &CliTestEnv, path: &str) -> Option<Output> {
    let binary = env.binary.to_string_lossy();
    let mut cmd = Command::new("script");
    if cfg!(target_os = "linux") {
        // util-linux: the command is one shell string; `-e` returns its exit status.
        let quoted = binary.replace('\'', r"'\''");
        cmd.args(["-qec", &format!("'{quoted}' list"), "/dev/null"]);
    } else {
        // BSD (macOS): the command and its arguments follow the output file.
        cmd.args(["-q", "/dev/null", &binary, "list"]);
    }
    let child = cmd
        .env("HOME", env.home.path())
        .env_remove("CLAUDE_CONFIG_DIR")
        .env("PATH", path)
        .env_remove("GRAPHMIND_SKIP_NOTICES")
        .env("GRAPHMIND_NO_AUTO_BUILD", "1")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn();
    let mut child = match child {
        Ok(child) => child,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return None,
        Err(e) => panic!("can't run `script`: {e}"),
    };
    let stdin = child.stdin.take();
    let out = child.wait_with_output().unwrap();
    drop(stdin);
    Some(out)
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

/// Without a terminal no memory clean runs: hooks get their daily upkeep from the SessionStart
/// hook, and nothing reaches the stdout a hook captures.
#[test]
fn no_memory_clean_without_a_terminal() {
    let env = CliTestEnv::new();
    current_setup(&env);
    let (path, _) = offline_curl(&env);

    let out = run_without_terminal(&env, &path);

    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(!env.home.path().join(".graphmind/memory-clean.stamp").exists(), "memory clean ran without a terminal");
    assert!(!stdout.contains("OK"), "clean summary on stdout: {stdout}");
}

/// In a terminal, an offline machine pays the fetch timeouts once per cache period, not on
/// every command.
#[test]
fn failed_fetches_are_cached() {
    let env = CliTestEnv::new();
    current_setup(&env);
    let (path, log) = offline_curl(&env);
    let Some(first) = run_in_terminal(&env, &path) else {
        eprintln!("failed_fetches_are_cached skipped: `script` not found, so no pseudo-terminal to run graphmind in");
        return;
    };
    assert!(first.status.success(), "{}", String::from_utf8_lossy(&first.stdout));
    let after_first = curl_calls(&log);
    assert!(after_first > 0, "interactive run should check for updates");

    let second = run_in_terminal(&env, &path).unwrap();
    assert!(second.status.success(), "{}", String::from_utf8_lossy(&second.stdout));
    assert_eq!(curl_calls(&log), after_first, "failed fetches retried on the next command");
}

/// Hooks run graphmind without a terminal, and they are how an upgrade reaches people who never
/// open one: an outdated setup still gets its auto-setup there, silently and in the background.
#[test]
fn auto_setup_runs_without_a_terminal() {
    let env = CliTestEnv::new();
    let config_path = env.home.path().join(".graphmind/config.json");
    let config = graphmind_config::GlobalConfig {
        setup_version: graphmind_config::SETUP_VERSION - 1,
        ..Default::default()
    };
    std::fs::write(&config_path, serde_json::to_string(&config).unwrap()).unwrap();
    // The detached `setup` inherits this PATH: system directories only (as in setup_e2e.rs), so
    // the machine's own `claude` CLI never runs and setup writes into the temp HOME.
    let (path, _) = offline_curl(&env);
    let own_output = env.run(&["list"]).stdout;

    let out = run_without_terminal(&env, &path);

    assert!(out.status.success());
    assert_eq!(String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&own_output), "auto-setup wrote to stdout");
    assert!(out.stderr.is_empty(), "auto-setup wrote to stderr: {}", String::from_utf8_lossy(&out.stderr));
    // Written before `setup` is spawned, so it exists once the command has exited.
    let stamp = std::fs::read_to_string(env.home.path().join(".graphmind/auto-setup.stamp"))
        .expect("no auto-setup.stamp: auto-setup didn't run");
    let fields: Vec<&str> = stamp.split_whitespace().collect();
    assert!(
        fields.len() == 3 && fields[0].parse::<u64>().is_ok() && fields[2].parse::<u64>().is_ok(),
        "stamp should be \"<unix ts> <setup version> <hooks fingerprint>\": {stamp:?}"
    );
    assert_eq!(fields[1], graphmind_config::SETUP_VERSION.to_string(), "stamp: {stamp:?}");

    // The detached setup ends by marking setup current. Waiting for that also keeps it from
    // outliving the temp HOME.
    let deadline = Instant::now() + Duration::from_secs(60);
    loop {
        let setup_version = std::fs::read_to_string(&config_path)
            .ok()
            .and_then(|raw| serde_json::from_str::<Value>(&raw).ok())
            .and_then(|cfg| cfg["setup_version"].as_u64());
        if setup_version == Some(graphmind_config::SETUP_VERSION.into()) {
            break;
        }
        assert!(Instant::now() < deadline, "background setup didn't finish within 60 s (setup_version {setup_version:?})");
        std::thread::sleep(Duration::from_millis(100));
    }
    assert!(env.home.path().join(".claude/CLAUDE.md").exists(), "setup didn't write the temp HOME's CLAUDE.md");
}

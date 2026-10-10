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
    // the machine's own `claude` CLI never runs and setup writes the temp HOME's ~/.claude.json.
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
    assert!(env.home.path().join(".claude.json").exists(), "setup didn't register Claude Code in the temp HOME");
}

// ---------------------------------------------------------------------------
// One setup per Claude Code folder (CLAUDE_CONFIG_DIR)
// ---------------------------------------------------------------------------

/// config.json with ~/.claude at `default_version`.
fn write_config(env: &CliTestEnv, default_version: u32) {
    let config = graphmind_config::GlobalConfig { setup_version: default_version, ..Default::default() };
    std::fs::write(env.home.path().join(".graphmind/config.json"), serde_json::to_string(&config).unwrap()).unwrap();
}

/// `graphmind <args>` without a terminal, as a hook or the MCP server of a Claude Code session in
/// `config_dir` (`None`: no CLAUDE_CONFIG_DIR) runs it. System directories only on PATH, so the
/// detached `setup` never runs the machine's own `claude` CLI.
fn run_in_session(env: &CliTestEnv, config_dir: Option<&Path>, args: &[&str]) -> Output {
    let mut cmd = Command::new(&env.binary);
    cmd.args(args)
        .env("HOME", env.home.path())
        .env("PATH", "/usr/bin:/bin")
        .env_remove("GRAPHMIND_SKIP_NOTICES")
        .env("GRAPHMIND_NO_AUTO_BUILD", "1")
        .stdin(Stdio::null());
    match config_dir {
        Some(dir) => cmd.env("CLAUDE_CONFIG_DIR", dir),
        None => cmd.env_remove("CLAUDE_CONFIG_DIR"),
    };
    cmd.output().unwrap()
}

/// Auto-setup stamps of CLAUDE_CONFIG_DIR folders (~/.claude's is `auto-setup.stamp`).
fn folder_stamps(env: &CliTestEnv) -> usize {
    std::fs::read_dir(env.home.path().join(".graphmind"))
        .unwrap()
        .flatten()
        .filter(|e| {
            let name = e.file_name().to_string_lossy().into_owned();
            name.starts_with("auto-setup-") && name.ends_with(".stamp")
        })
        .count()
}

/// Wait for the detached `setup` to mark `dir` set up, so it is done before the temp HOME goes.
fn wait_for_setup_of(env: &CliTestEnv, dir: &Path) {
    let config = env.home.path().join(".graphmind/config.json");
    let key = dir.to_string_lossy().into_owned();
    let done = || {
        std::fs::read_to_string(&config)
            .ok()
            .and_then(|raw| serde_json::from_str::<graphmind_config::GlobalConfig>(&raw).ok())
            .is_some_and(|c| c.setup_versions.get(&key) == Some(&graphmind_config::SETUP_VERSION))
    };
    let deadline = Instant::now() + Duration::from_secs(60);
    while !done() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(200));
    }
    assert!(done(), "auto-setup didn't set up {key}");
}

/// A session in a CLAUDE_CONFIG_DIR folder that isn't set up gets its own auto-setup, although
/// ~/.claude is current and its stamp is fresh: neither the version nor the throttle is shared.
#[test]
fn auto_setup_runs_for_an_outdated_claude_folder_while_the_default_is_current() {
    let env = CliTestEnv::new();
    write_config(&env, graphmind_config::SETUP_VERSION);
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs();
    let fingerprint = graphmind_cli::commands::claude_hook::hooks_fingerprint();
    let default_stamp = env.home.path().join(".graphmind/auto-setup.stamp");
    let stamp = format!("{now} {} {fingerprint}", graphmind_config::SETUP_VERSION);
    std::fs::write(&default_stamp, &stamp).unwrap();
    let work = env.home.path().join("profiles/work");
    std::fs::create_dir_all(&work).unwrap();

    let out = run_in_session(&env, Some(&work), &["list"]);

    assert!(out.status.success());
    assert!(out.stderr.is_empty(), "auto-setup wrote to stderr: {}", String::from_utf8_lossy(&out.stderr));
    assert_eq!(folder_stamps(&env), 1, "no auto-setup for the folder");
    assert_eq!(std::fs::read_to_string(&default_stamp).unwrap(), stamp, "~/.claude's stamp rewritten");
    wait_for_setup_of(&env, &work);
    let settings = std::fs::read_to_string(work.join("settings.json")).unwrap();
    assert!(settings.contains(&*work.join("hooks/graphmind-session.sh").to_string_lossy()), "{settings}");
    assert!(!env.home.path().join(".claude/hooks").exists(), "~/.claude set up instead");
}

/// A folder without graphmind hooks has nothing to start auto-setup but the MCP server, so the
/// server sets up its session's CLAUDE_CONFIG_DIR folder once setup has run somewhere.
#[test]
fn mcp_server_sets_up_a_new_claude_folder() {
    let env = CliTestEnv::new();
    write_config(&env, graphmind_config::SETUP_VERSION);
    let work = env.home.path().join("profiles/work");
    std::fs::create_dir_all(&work).unwrap();

    let out = run_in_session(&env, Some(&work), &["mcp"]);

    assert!(out.stdout.is_empty(), "auto-setup wrote to the protocol stream: {}", String::from_utf8_lossy(&out.stdout));
    assert_eq!(folder_stamps(&env), 1, "no auto-setup from the MCP server");
    wait_for_setup_of(&env, &work);
    assert!(work.join("hooks/graphmind-prompt.sh").exists());
}

/// The MCP servers Cursor, OpenCode or Claude Desktop start have no CLAUDE_CONFIG_DIR: they never
/// set up ~/.claude, even an outdated one (its hooks do that). And nobody who never ran setup gets
/// one from an MCP server.
#[test]
fn mcp_server_leaves_the_default_folder_and_first_installs_alone() {
    let env = CliTestEnv::new();
    write_config(&env, graphmind_config::SETUP_VERSION - 1);
    run_in_session(&env, None, &["mcp"]);
    assert!(!env.home.path().join(".graphmind/auto-setup.stamp").exists(), "MCP server set up ~/.claude");

    write_config(&env, 0);
    let work = env.home.path().join("profiles/work");
    std::fs::create_dir_all(&work).unwrap();
    run_in_session(&env, Some(&work), &["mcp"]);
    assert_eq!(folder_stamps(&env), 0, "MCP server set up a folder for someone who never ran setup");
    assert!(!work.join("hooks").exists());
}

//! An unreadable config.json is never replaced by defaults.

mod common;
use common::CliTestEnv;
use std::process::Command;

fn config_path(env: &CliTestEnv) -> std::path::PathBuf {
    env.home.path().join(".graphmind").join("config.json")
}

/// A valid config followed by stray bytes (the tail of an interrupted concurrent write) still
/// reads: projects are there and nothing is written. The next save rewrites the whole file.
#[test]
fn trailing_bytes_are_tolerated_and_dropped_by_the_next_save() {
    let env = CliTestEnv::new();
    let fixture = env.fixture_path();
    env.run_ok(&["register", fixture.to_str().unwrap(), "--slug", "keep-me"]);
    let path = config_path(&env);
    let mut raw = std::fs::read_to_string(&path).unwrap();
    raw.push_str("aw");
    std::fs::write(&path, &raw).unwrap();

    let list = env.run_ok(&["list"]);
    assert!(list.contains("keep-me"), "project lost: {list}");
    assert_eq!(std::fs::read_to_string(&path).unwrap(), raw, "reading must not write");

    env.run_ok(&["exclude", "add", "generated", "--global"]);
    let saved = std::fs::read_to_string(&path).unwrap();
    serde_json::from_str::<serde_json::Value>(&saved).expect("the save writes valid JSON");
    assert!(saved.contains("keep-me") && saved.contains("generated"), "{saved}");
}

/// An unparseable config is never replaced by defaults: every writer refuses (the registry's
/// read-modify-write and a plain load/edit/save like `exclude add`), and the bytes survive.
#[test]
fn unreadable_config_is_never_overwritten() {
    let env = CliTestEnv::new();
    let path = config_path(&env);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    let garbage = "{\"projects\": {\"precious\": oops";
    std::fs::write(&path, garbage).unwrap();

    let fixture = env.fixture_path();
    env.run(&["register", fixture.to_str().unwrap(), "--slug", "would-wipe"]);
    let out = env.run(&["exclude", "add", "generated", "--global"]);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("not saving"), "expected a refusal, got: {stderr}");
    assert_eq!(std::fs::read_to_string(&path).unwrap(), garbage, "unreadable config was overwritten");
}

/// An unreadable config reads as setup_version 0; that must not trigger the automatic setup,
/// which ends by saving the config.
#[test]
fn unreadable_config_does_not_trigger_auto_setup() {
    let env = CliTestEnv::new();
    let path = config_path(&env);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, "not json at all").unwrap();

    // Run without GRAPHMIND_SKIP_NOTICES so the notice/auto-setup path executes.
    let out = Command::new(&env.binary)
        .args(["list"])
        .env("HOME", env.home.path())
        .output()
        .expect("failed to run graphmind");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(!stderr.contains("outdated"), "setup notice on an unreadable config: {stderr}");
    // Auto-setup runs as a detached process, after writing this stamp: its absence shows none
    // was started. Then give a stray setup time to finish before checking what it would write.
    let stamp = env.home.path().join(".graphmind").join("auto-setup.stamp");
    assert!(!stamp.exists(), "auto-setup was started on an unreadable config");
    std::thread::sleep(std::time::Duration::from_millis(1500));
    assert!(!env.home.path().join(".claude").join("hooks").exists(), "auto-setup wrote Claude hooks");
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "not json at all");
}

/// A valid config is left exactly as it is by commands that only read it.
#[test]
fn valid_config_untouched() {
    let env = CliTestEnv::new();
    let fixture = env.fixture_path();
    env.run_ok(&["register", fixture.to_str().unwrap(), "--slug", "fine"]);
    let before = std::fs::read_to_string(config_path(&env)).unwrap();
    env.run_ok(&["list"]);
    assert_eq!(std::fs::read_to_string(config_path(&env)).unwrap(), before);
}

/// An empty or NUL-filled config.json (a `touch`, a crash mid-write by an older version) reads
/// as defaults and doesn't block saves.
#[test]
fn empty_config_reads_as_defaults_and_saves() {
    let env = CliTestEnv::new();
    let path = config_path(&env);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    for raw in ["", "\0\0\0\0"] {
        std::fs::write(&path, raw).unwrap();
        env.run_ok(&["exclude", "add", "generated", "--global"]);
        assert!(std::fs::read_to_string(&path).unwrap().contains("generated"), "{raw:?}");
    }
}

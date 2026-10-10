//! E2E tests for when setup counts as done: `setup_version`, which the CLI and the desktop compare
//! with SETUP_VERSION, and the hook registrations auto-setup checks, so hooks settings.json doesn't
//! register are retried whatever `setup_version` says.
//!
//! Run with:
//!   cargo test -p graphmind-cli --test setup_state_e2e -- --test-threads=1

use common::with_home;
use graphmind_cli::commands::claude_hook::{install_hook, installed_hooks_outdated};
use graphmind_cli::commands::setup::setup;
use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};

mod common;

fn settings_json(home: &Path) -> PathBuf {
    home.join(".claude").join("settings.json")
}

/// graphmind's SessionStart hook is registered in settings.json.
fn hooks_registered(home: &Path) -> bool {
    let json: Value = serde_json::from_str(&fs::read_to_string(settings_json(home)).unwrap()).unwrap();
    json["hooks"]["SessionStart"]
        .as_array()
        .into_iter()
        .flatten()
        .any(|entry| entry["hooks"].to_string().contains("graphmind-session.sh"))
}

// ---------------------------------------------------------------------------
// 12. Setup version — stamped only once the Claude Code entry is in place
// ---------------------------------------------------------------------------

#[test]
fn setup_version_waits_for_the_claude_code_entry() {
    with_home(|home| {
        let setup_version = || graphmind_config::read_config().unwrap().setup_version;
        // Claude Code caught mid-write: the user-scope entry can't be added.
        fs::write(home.join(".claude.json"), r#"{"numStartups": 3,"#).unwrap();
        setup();
        assert_eq!(setup_version(), 0, "auto-setup must retry while the entry is missing");

        fs::write(home.join(".claude.json"), r#"{"numStartups": 3}"#).unwrap();
        setup();
        assert_eq!(setup_version(), graphmind_config::SETUP_VERSION);
    });
}

/// While setup isn't marked complete, its summary doesn't open with "Setup complete".
#[test]
fn setup_summary_says_finished_while_the_claude_code_entry_is_missing() {
    with_home(|home| {
        fs::write(home.join(".claude.json"), r#"{"numStartups": 3,"#).unwrap();
        let out = common::run_setup_as(Path::new(env!("CARGO_BIN_EXE_graphmind")), home, "/bin/zsh");
        let stdout = String::from_utf8_lossy(&out.stdout);
        assert!(out.status.success(), "setup failed\nstdout: {stdout}\nstderr: {}", String::from_utf8_lossy(&out.stderr));
        assert!(stdout.contains("Setup finished"), "{stdout}");
        assert!(!stdout.contains("Setup complete"), "{stdout}");
        assert!(stdout.contains("setup not marked complete"), "{stdout}");
    });
}

// ---------------------------------------------------------------------------
// 17. Hook registration — auto-setup retries hooks settings.json doesn't register
// ---------------------------------------------------------------------------

/// A completed install whose settings.json later loses graphmind's hooks (rewritten by hand or by
/// another tool). `setup_version` is already current, so only the registration check can start
/// auto-setup again.
#[test]
fn lost_hook_registrations_trigger_auto_setup_after_a_completed_setup() {
    with_home(|home| {
        setup();
        assert_eq!(graphmind_config::read_config().unwrap().setup_version, graphmind_config::SETUP_VERSION);
        assert!(!installed_hooks_outdated(), "a fresh install is current");

        fs::write(settings_json(home), r#"{"permissions": {}}"#).unwrap();
        assert!(installed_hooks_outdated(), "lost registrations must start auto-setup");

        setup();
        assert!(hooks_registered(home));
        assert!(!installed_hooks_outdated());
    });
}

/// A first install with a settings.json that doesn't parse: setup leaves the file alone but writes
/// the hook scripts and stamps `setup_version`, so neither the CLI nor the desktop reports an
/// outdated setup that running setup can't fix. A retry can't do better while the file doesn't
/// parse, so the trigger stays quiet; once the file is fixed, the unregistered scripts start
/// auto-setup, which registers them.
#[test]
fn hooks_left_unregistered_by_a_broken_settings_json_are_retried_once_it_is_fixed() {
    with_home(|home| {
        let settings = settings_json(home);
        fs::create_dir_all(settings.parent().unwrap()).unwrap();
        fs::write(&settings, r#"{"permissions": {},}"#).unwrap();

        setup();
        assert_eq!(fs::read_to_string(&settings).unwrap(), r#"{"permissions": {},}"#);
        assert_eq!(graphmind_config::read_config().unwrap().setup_version, graphmind_config::SETUP_VERSION);
        assert!(!installed_hooks_outdated(), "nothing to retry while settings.json doesn't parse");

        fs::write(&settings, r#"{"permissions": {}}"#).unwrap();
        assert!(installed_hooks_outdated(), "a fixed settings.json without the hooks must start auto-setup");

        setup();
        assert!(hooks_registered(home));
        assert!(!installed_hooks_outdated());
    });
}

/// A settings.json whose `"hooks"` isn't an object can't take the hooks, and the user keeps it:
/// the trigger stays on (auto-setup's stamp throttles the retries to one a day), while
/// `setup_version` is stamped, so no "outdated" notice asks for a setup that can't help.
#[test]
fn hooks_setup_cant_register_keep_the_trigger_on_but_not_the_outdated_notice() {
    with_home(|home| {
        let settings = settings_json(home);
        fs::create_dir_all(settings.parent().unwrap()).unwrap();
        fs::write(&settings, r#"{"hooks": []}"#).unwrap();

        setup();
        assert_eq!(fs::read_to_string(&settings).unwrap(), r#"{"hooks": []}"#);
        assert_eq!(graphmind_config::read_config().unwrap().setup_version, graphmind_config::SETUP_VERSION);
        assert!(installed_hooks_outdated());
    });
}

/// A first hook install that stops at a script it can't write (a folder in its place) registers
/// nothing. The scripts written before it start auto-setup, which finishes once the path is free.
#[test]
fn a_hook_install_stopped_by_a_script_it_cant_write_is_retried() {
    with_home(|home| {
        let hooks = home.join(".claude").join("hooks");
        let blocker = hooks.join("graphmind-session.sh");
        fs::create_dir_all(&blocker).unwrap();

        assert!(install_hook().is_err());
        assert!(hooks.join("graphmind-search.sh").is_file(), "scripts before the blocked one are written");
        assert!(installed_hooks_outdated(), "a half-done install must start auto-setup");

        fs::remove_dir(&blocker).unwrap();
        setup();
        assert!(hooks_registered(home));
        assert!(!installed_hooks_outdated());
    });
}

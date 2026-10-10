//! `graphmind install hook-claude` writes the current hook set and migrates older installs.

mod common;
use common::CliTestEnv;
use serde_json::{json, Value};

#[test]
fn install_migrates_old_entries_and_keeps_other_hooks() {
    let env = CliTestEnv::new();
    let claude = env.home.path().join(".claude");
    let hooks = claude.join("hooks");
    std::fs::create_dir_all(&hooks).unwrap();
    for old in ["graphmind-post.sh", "graphmind-stop.sh"] {
        std::fs::write(hooks.join(old), "#!/bin/sh\n").unwrap();
    }
    let gm = |name: &str| hooks.join(name).to_string_lossy().to_string();
    let old_settings = json!({
        "model": "opus",
        "hooks": {
            "PreToolUse": [
                {"matcher": "Bash", "hooks": [
                    {"type": "command", "command": "/usr/local/bin/my-guard.py"},
                    {"type": "command", "command": gm("graphmind-search.sh")}
                ]},
                {"matcher": "Glob", "hooks": [{"type": "command", "command": gm("graphmind-search.sh")}]},
                {"matcher": "LS", "hooks": [{"type": "command", "command": gm("graphmind-search.sh")}]}
            ],
            "PostToolUse": [{"matcher": "Grep", "hooks": [{"type": "command", "command": gm("graphmind-post.sh")}]}],
            "Stop": [{"matcher": "*", "hooks": [{"type": "command", "command": gm("graphmind-stop.sh")}]}]
        }
    });
    std::fs::write(claude.join("settings.json"), old_settings.to_string()).unwrap();

    env.run_ok(&["install", "hook-claude"]);
    env.run_ok(&["install", "hook-claude"]); // idempotent

    let settings: Value = serde_json::from_str(&std::fs::read_to_string(claude.join("settings.json")).unwrap()).unwrap();
    let h = &settings["hooks"];
    assert_eq!(settings["model"], "opus", "unrelated settings kept");
    assert!(h.get("PostToolUse").is_none(), "retired PostToolUse hook removed: {h}");
    assert!(h.get("Stop").is_none(), "retired Stop hook removed: {h}");

    let pre = h["PreToolUse"].as_array().unwrap();
    let matchers: Vec<_> = pre.iter().map(|e| e["matcher"].as_str().unwrap()).collect();
    assert_eq!(matchers, ["Bash", "Grep", "Glob", "Agent"], "{pre:?}");
    let bash_cmds: Vec<_> = pre[0]["hooks"].as_array().unwrap().iter().map(|x| x["command"].as_str().unwrap()).collect();
    assert_eq!(bash_cmds, ["/usr/local/bin/my-guard.py", gm("graphmind-search.sh").as_str()]);
    for event in ["SessionStart", "UserPromptSubmit"] {
        assert_eq!(h[event].as_array().unwrap().len(), 1, "{event} registered once");
        assert!(h[event][0]["hooks"][0]["timeout"].as_u64().is_some(), "{event} has a timeout");
    }

    assert!(!hooks.join("graphmind-post.sh").exists());
    assert!(!hooks.join("graphmind-stop.sh").exists());
    for script in ["graphmind-search.sh", "graphmind-session.sh", "graphmind-prompt.sh"] {
        let body = std::fs::read_to_string(hooks.join(script)).unwrap();
        // Hooks run without a terminal, which is what keeps notices out of them; no env var.
        assert!(!body.contains("GRAPHMIND_HOOK") && !body.contains("GRAPHMIND_SKIP_NOTICES"), "{script} sets no mode variable");
        assert!(body.contains("graphmind which"), "{script} uses the fast project check");
    }
}

#[test]
fn which_prints_slug_only_inside_a_registered_project() {
    let env = CliTestEnv::new();
    let fixture = env.fixture_path();
    env.run_ok(&["register", fixture.to_str().unwrap(), "--slug", "which-test"]);

    let run_in = |dir: &std::path::Path| {
        std::process::Command::new(&env.binary)
            .arg("which")
            .current_dir(dir)
            .env("HOME", env.home.path())
            .output()
            .unwrap()
    };

    let inside = run_in(&fixture.join("src"));
    assert!(inside.status.success());
    assert_eq!(String::from_utf8_lossy(&inside.stdout).trim(), "which-test");

    // No "the only registered project" fallback: outside the project, exit 1.
    let outside = run_in(env.home.path());
    assert!(!outside.status.success());
    assert!(outside.stdout.is_empty() && outside.stderr.is_empty(), "silent: no notices");
}

/// Config at the current setup version, so only the hook scripts can make auto-setup run.
fn current_setup(env: &CliTestEnv) {
    let fixture = env.fixture_path();
    env.run_ok(&["register", fixture.to_str().unwrap(), "--slug", "hooks-test"]);
    let path = env.home.path().join(".graphmind/config.json");
    let mut cfg: Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    cfg["setup_version"] = json!(graphmind_config::SETUP_VERSION);
    std::fs::write(&path, cfg.to_string()).unwrap();
}

/// Any quick command with notices enabled, as for a real user after an upgrade.
fn run_with_notices(env: &CliTestEnv) {
    let out = std::process::Command::new(&env.binary)
        .arg("list")
        .env("HOME", env.home.path())
        .env("GRAPHMIND_NO_AUTO_BUILD", "1")
        .output()
        .unwrap();
    assert!(out.status.success());
}

/// A hook script changed in a new binary reaches existing installs without a SETUP_VERSION bump.
#[test]
fn outdated_hook_script_is_reinstalled_by_auto_setup() {
    let env = CliTestEnv::new();
    current_setup(&env);
    env.run_ok(&["install", "hook-claude"]);
    let prompt = env.home.path().join(".claude/hooks/graphmind-prompt.sh");
    let old = "#!/usr/bin/env bash\n# graphmind prompt hook from an older version\n";
    std::fs::write(&prompt, old).unwrap();

    run_with_notices(&env);

    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(60);
    while std::fs::read_to_string(&prompt).unwrap_or_default() == old && std::time::Instant::now() < deadline {
        std::thread::sleep(std::time::Duration::from_millis(200));
    }
    let now = std::fs::read_to_string(&prompt).unwrap();
    assert!(now != old && now.contains("graphmind"), "auto-setup should reinstall the prompt hook");
}

/// No hooks installed and setup current: nothing to do, and hooks are never added unasked.
#[test]
fn no_auto_setup_without_installed_hooks() {
    let env = CliTestEnv::new();
    current_setup(&env);

    run_with_notices(&env);

    std::thread::sleep(std::time::Duration::from_millis(1500));
    assert!(!env.home.path().join(".graphmind/auto-setup.stamp").exists(), "auto-setup started");
    assert!(!env.home.path().join(".claude/hooks/graphmind-prompt.sh").exists(), "hooks added unasked");
}

/// Runs an installed hook script the way Claude Code does: JSON on stdin, cwd in the project.
fn run_hook(env: &CliTestEnv, script: &str, cwd: &std::path::Path, input: &Value) -> String {
    use std::io::Write;
    let mut child = std::process::Command::new("bash")
        .arg(env.home.path().join(".claude/hooks").join(script))
        .current_dir(cwd)
        .env("HOME", env.home.path())
        .env("GRAPHMIND_NO_AUTO_BUILD", "1")
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(input.to_string().as_bytes()).unwrap();
    let out = child.wait_with_output().unwrap();
    String::from_utf8_lossy(&out.stdout).to_string()
}

/// The search hook nudges toward graphmind for the ways Claude Code searches: the Grep and Glob
/// tools where they exist, and search commands through Bash, including the ugrep/bfs native
/// builds use and searches after `cd … &&`. Output filtered through a pipe isn't a code search.
#[test]
fn search_hook_nudges_for_code_searches() {
    if std::process::Command::new("jq").arg("--version").output().is_err() {
        return; // the hooks need jq
    }
    let env = CliTestEnv::new();
    current_setup(&env);
    let project = env.fixture_path();
    env.run_ok(&["install", "hook-claude"]);
    let cases = [
        (json!({"tool_name": "Grep", "tool_input": {"pattern": "x"}}), true),
        (json!({"tool_name": "Glob", "tool_input": {"pattern": "**/*.rs"}}), true),
        (json!({"tool_name": "Bash", "tool_input": {"command": "rg fetchInBatches src"}}), true),
        (json!({"tool_name": "Bash", "tool_input": {"command": "cd src && rg fetchInBatches"}}), true),
        (json!({"tool_name": "Bash", "tool_input": {"command": "cd /tmp/graphmind-x && rg foo"}}), true),
        (json!({"tool_name": "Bash", "tool_input": {"command": "test -d src || rg fetchUser"}}), true),
        (json!({"tool_name": "Bash", "tool_input": {"command": "rg graphmind_config src"}}), true),
        (json!({"tool_name": "Bash", "tool_input": {"command": "ugrep -r fetchInBatches ."}}), true),
        (json!({"tool_name": "Bash", "tool_input": {"command": "bfs . -name '*.rs'"}}), true),
        (json!({"tool_name": "Agent", "tool_input": {"subagent_type": "Explore", "prompt": "list the modules"}}), true),
        (json!({"tool_name": "Bash", "tool_input": {"command": "cargo test 2>&1 | grep FAILED"}}), false),
        (json!({"tool_name": "Bash", "tool_input": {"command": "graphmind search fetch"}}), false),
        (json!({"tool_name": "Bash", "tool_input": {"command": "cd /tmp/graphmind-x && graphmind search fetch"}}), false),
        (json!({"tool_name": "Read", "tool_input": {"file_path": "a.rs"}}), false),
    ];
    for (i, (mut input, nudges)) in cases.into_iter().enumerate() {
        input["session_id"] = json!(format!("search-{i}")); // once per session: a fresh one each
        let out = run_hook(&env, "graphmind-search.sh", &project, &input);
        assert_eq!(out.contains("graphmind-indexed"), nudges, "{input}: {out}");
    }
}

/// Every PreToolUse matcher the install registers reaches a nudge for some input, so a matcher
/// added to HOOKS without a case in the script (or the other way round) fails here.
#[test]
fn every_search_hook_matcher_nudges() {
    if std::process::Command::new("jq").arg("--version").output().is_err() {
        return; // the hooks need jq
    }
    let env = CliTestEnv::new();
    current_setup(&env);
    let project = env.fixture_path();
    env.run_ok(&["install", "hook-claude"]);
    let settings = read_settings(&env);
    let matchers: Vec<String> = settings["hooks"]["PreToolUse"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["matcher"].as_str().unwrap().to_string())
        .collect();
    assert!(!matchers.is_empty(), "{settings}");
    for (i, matcher) in matchers.iter().enumerate() {
        let mut input = match matcher.as_str() {
            "Grep" => json!({"tool_name": "Grep", "tool_input": {"pattern": "x"}}),
            "Glob" => json!({"tool_name": "Glob", "tool_input": {"pattern": "**/*.rs"}}),
            "Agent" => json!({"tool_name": "Agent", "tool_input": {"subagent_type": "Explore", "prompt": "x"}}),
            "Bash" => json!({"tool_name": "Bash", "tool_input": {"command": "rg fetchInBatches src"}}),
            other => panic!("no sample input for the {other} matcher: add one here and a case to the script"),
        };
        input["session_id"] = json!(format!("matcher-{i}"));
        let out = run_hook(&env, "graphmind-search.sh", &project, &input);
        assert!(out.contains("graphmind-indexed"), "{matcher} does not nudge: {out}");
    }
}

fn read_settings(env: &CliTestEnv) -> Value {
    serde_json::from_str(&std::fs::read_to_string(env.home.path().join(".claude/settings.json")).unwrap()).unwrap()
}

/// settings.json that lost a registration is repaired by auto-setup even though the hook scripts
/// are unchanged: the Glob matcher is removed by hand here.
#[test]
fn missing_registration_is_repaired_by_auto_setup() {
    let env = CliTestEnv::new();
    current_setup(&env);
    env.run_ok(&["install", "hook-claude"]);
    let path = env.home.path().join(".claude/settings.json");
    let mut settings = read_settings(&env);
    settings["hooks"]["PreToolUse"].as_array_mut().unwrap().retain(|e| e["matcher"] != "Glob");
    std::fs::write(&path, settings.to_string()).unwrap();
    let has_glob = |env: &CliTestEnv| read_settings(env)["hooks"]["PreToolUse"].as_array().unwrap().iter().any(|e| e["matcher"] == "Glob");
    assert!(!has_glob(&env));

    run_with_notices(&env);

    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(60);
    while !has_glob(&env) && std::time::Instant::now() < deadline {
        std::thread::sleep(std::time::Duration::from_millis(200));
    }
    assert!(has_glob(&env), "auto-setup should re-register the Glob matcher");
}

/// The prompt hook doesn't repeat priority entries SessionStart showed, even ones longer than
/// either hook's cut, and leaves them out of the related memories it finds.
#[test]
fn prompt_hook_dedupes_priority_entries_by_id() {
    if std::process::Command::new("jq").arg("--version").output().is_err() {
        return; // the hooks need jq
    }
    let env = CliTestEnv::new();
    current_setup(&env);
    let project = env.fixture_path();
    env.run_ok(&["install", "hook-claude"]);
    let long = "x".repeat(400);
    env.run_ok(&["memory", "add", &format!("deploy pipeline uses terraform apply {long}"), "--slug", "hooks-test", "--priority"]);
    env.run_ok(&["memory", "add", &format!("deploy pipeline needs the staging account {long}"), "--slug", "hooks-test", "--priority"]);

    let input = json!({"session_id": "s1", "prompt": "how does the deploy pipeline terraform step work"});
    let start = run_hook(&env, "graphmind-session.sh", &project, &input);
    assert!(start.contains("terraform apply"), "SessionStart shows priority context: {start}");

    let prompt = run_hook(&env, "graphmind-prompt.sh", &project, &input);
    assert!(!prompt.contains("new priority context"), "priority entries repeated: {prompt}");
    assert!(!prompt.contains("terraform apply"), "already-shown entry returned as related: {prompt}");

    // A priority entry added mid-session is shown once.
    env.run_ok(&["memory", "add", "release checklist lives in RELEASING.md", "--slug", "hooks-test", "--priority"]);
    let next = run_hook(&env, "graphmind-prompt.sh", &project, &input);
    assert!(next.contains("RELEASING.md"), "new priority entry shown: {next}");
    let again = run_hook(&env, "graphmind-prompt.sh", &project, &input);
    assert!(!again.contains("RELEASING.md"), "new priority entry shown only once: {again}");
}

/// A priority entry cut from the output to fit the size limit isn't recorded as shown, so a later
/// prompt still offers it. Both hooks' limits are lowered in the installed copies to force a cut.
#[test]
fn priority_entries_cut_for_size_are_offered_again() {
    if std::process::Command::new("jq").arg("--version").output().is_err() {
        return; // the hooks need jq
    }
    let env = CliTestEnv::new();
    current_setup(&env);
    let project = env.fixture_path();
    env.run_ok(&["install", "hook-claude"]);
    let hooks = env.home.path().join(".claude/hooks");
    let set_limit = |script: &str, chars: u32| {
        let path = hooks.join(script);
        let text = std::fs::read_to_string(&path).unwrap();
        let line = text.lines().find(|l| l.starts_with("MAX_CHARS=")).unwrap().to_string();
        std::fs::write(&path, text.replace(&line, &format!("MAX_CHARS={chars}"))).unwrap();
    };
    let long = "x".repeat(400);
    env.run_ok(&["memory", "add", &format!("deploy pipeline uses terraform apply {long}"), "--slug", "hooks-test", "--priority"]);
    env.run_ok(&["memory", "add", &format!("deploy pipeline needs the staging account {long}"), "--slug", "hooks-test", "--priority"]);
    let input = json!({"session_id": "s2", "prompt": "how does the deploy pipeline terraform step work"});

    set_limit("graphmind-session.sh", 40);
    let start = run_hook(&env, "graphmind-session.sh", &project, &input);
    assert!(!start.contains("terraform apply"), "SessionStart cut before the priority block: {start}");

    // The prompt hook's priority block doesn't fit either, so it's dropped and not recorded.
    set_limit("graphmind-prompt.sh", 200);
    let dropped = run_hook(&env, "graphmind-prompt.sh", &project, &input);
    assert!(!dropped.contains("new priority context"), "{dropped}");

    set_limit("graphmind-prompt.sh", 9000);
    let next = run_hook(&env, "graphmind-prompt.sh", &project, &input);
    assert!(next.contains("new priority context") && next.contains("terraform apply"), "cut entries offered again: {next}");
    let again = run_hook(&env, "graphmind-prompt.sh", &project, &input);
    assert!(!again.contains("new priority context"), "shown once, then recorded: {again}");
}

/// Without a terminal (hooks, scripts) auto-setup still runs, but no notices are printed, and no
/// environment variable is needed for that.
#[test]
fn hooks_run_auto_setup_without_notices() {
    let env = CliTestEnv::new();
    current_setup(&env);
    let path = env.home.path().join(".graphmind/config.json");
    let mut cfg: Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    cfg["setup_version"] = json!(1);
    std::fs::write(&path, cfg.to_string()).unwrap();

    let out = std::process::Command::new(&env.binary)
        .arg("list")
        .env("HOME", env.home.path())
        .env("GRAPHMIND_NO_AUTO_BUILD", "1")
        .output()
        .unwrap();
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(!stderr.contains("outdated"), "no notices without a terminal: {stderr}");
    assert!(env.home.path().join(".graphmind/auto-setup.stamp").exists(), "auto-setup started from the hook");
}

/// `memory show` resolves a worktree to its main project's memory, like list and search.
#[test]
fn memory_show_works_in_a_worktree() {
    let env = CliTestEnv::new();
    let git = |dir: &std::path::Path, args: &[&str]| {
        let ok = std::process::Command::new("git").arg("-C").arg(dir).args(args).env("HOME", env.home.path()).status().unwrap();
        assert!(ok.success(), "git {args:?}");
    };
    let repo = env.home.path().join("app");
    std::fs::create_dir_all(repo.join("src")).unwrap();
    std::fs::write(repo.join("src/a.ts"), "export function a() {}\n").unwrap();
    git(&repo, &["init", "-q"]);
    git(&repo, &["-c", "user.email=t@t", "-c", "user.name=t", "add", "."]);
    git(&repo, &["-c", "user.email=t@t", "-c", "user.name=t", "commit", "-q", "-m", "init"]);
    env.run_ok(&["register", repo.to_str().unwrap(), "--slug", "app"]);
    let added = env.run_ok(&["memory", "add", "worktree memory fact", "--slug", "app"]);
    let id: String = added.split_whitespace().find(|w| w.len() >= 8 && w.chars().all(|c| c.is_ascii_hexdigit() || c == '-')).unwrap()[..8].to_string();

    let wt = env.home.path().join("app-feature");
    git(&repo, &["worktree", "add", "-q", "-b", "feature", wt.to_str().unwrap()]);
    let shown = env.run_ok_in(&wt, &["memory", "show", &id]);
    assert!(shown.contains("worktree memory fact"), "got: {shown}");
}

/// The prompt hook's automatic memory search doesn't count as a recall (`--no-recall`), so it
/// can't push entries to auto-promotion (issue #112).
#[test]
fn prompt_hook_searches_do_not_count_recalls() {
    if std::process::Command::new("jq").arg("--version").output().is_err() {
        return; // the hooks need jq
    }
    let env = CliTestEnv::new();
    current_setup(&env);
    let project = env.fixture_path();
    env.run_ok(&["install", "hook-claude"]);
    env.run_ok(&["memory", "add", "the zeppelin cache keeps entries for an hour", "--slug", "hooks-test"]);

    let input = json!({"session_id": "s1", "prompt": "how long does the zeppelin cache keep entries"});
    let prompt = run_hook(&env, "graphmind-prompt.sh", &project, &input);
    assert!(prompt.contains("zeppelin cache keeps"), "precondition: the hook found the entry: {prompt}");

    let memory_dir = env.home.path().join(".graphmind/memory");
    let entry: Value = std::fs::read_dir(&memory_dir)
        .unwrap()
        .flatten()
        .filter(|f| f.path().extension().is_some_and(|e| e == "jsonl"))
        .flat_map(|f| std::fs::read_to_string(f.path()).unwrap().lines().map(String::from).collect::<Vec<_>>())
        .filter_map(|line| serde_json::from_str::<Value>(&line).ok())
        .find(|e| e["content"].as_str().is_some_and(|c| c.contains("zeppelin")))
        .unwrap();
    assert_eq!(entry["recall_count"], 0, "{entry}");
}

//! Claude Code reads graphmind per project, from `projects.<path>.mcpServers.graphmind` in
//! .claude.json: `init` and `register` add the entry, `unregister` removes it, `setup` refreshes
//! stale entries and replaces a user-scope entry, `uninstall all` removes them. Every test runs the
//! graphmind binary with a temp HOME and a PATH holding only the system directories and
//! `<home>/bin` (where a test may put a fake `claude`).
//!
//! Run with:
//!   cargo test -p graphmind-cli --test setup_claude_code_mcp_e2e -- --test-threads=1

mod common;
use common::{backups_of, CliTestEnv};
use serde_json::{json, Value};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

/// `graphmind <args>` in `cwd`, with `extra_env` set.
fn graphmind(env: &CliTestEnv, cwd: &Path, args: &[&str], extra_env: &[(&str, &Path)]) -> Output {
    let path = std::env::join_paths([env.home.path().join("bin"), "/usr/bin".into(), "/bin".into()]).unwrap();
    let mut command = Command::new(&env.binary);
    command
        .args(args)
        .current_dir(cwd)
        .env("HOME", env.home.path())
        .env("PATH", path)
        .env_remove("CLAUDE_CONFIG_DIR")
        .env("GRAPHMIND_SKIP_NOTICES", "1")
        .env("GRAPHMIND_NO_AUTO_BUILD", "1");
    for (key, value) in extra_env {
        command.env(key, value);
    }
    command.output().expect("failed to run graphmind")
}

fn graphmind_ok(env: &CliTestEnv, cwd: &Path, args: &[&str]) -> String {
    let out = graphmind(env, cwd, args, &[]);
    let stdout = String::from_utf8_lossy(&out.stdout).to_string();
    assert!(out.status.success(), "{args:?} failed\nstdout: {stdout}\nstderr: {}", String::from_utf8_lossy(&out.stderr));
    stdout
}

fn git(dir: &Path, args: &[&str]) {
    let out = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(["-c", "user.name=test", "-c", "user.email=test@test.com", "-c", "commit.gpgsign=false"])
        .args(args)
        .output()
        .expect("failed to run git");
    assert!(out.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&out.stderr));
}

/// A git repo at `<home>/<name>` with one commit; its canonical path (the registry's key).
fn repo(env: &CliTestEnv, name: &str) -> PathBuf {
    let repo = env.home.path().join(name);
    fs::create_dir_all(&repo).unwrap();
    git(&repo, &["init", "-q"]);
    fs::write(repo.join("lib.rs"), "pub fn f() {}\n").unwrap();
    git(&repo, &["add", "."]);
    git(&repo, &["commit", "-q", "-m", "init"]);
    repo.canonicalize().unwrap()
}

/// A plain folder registered with graphmind; its canonical path.
fn registered_folder(env: &CliTestEnv, name: &str) -> PathBuf {
    let dir = env.home.path().join(name);
    fs::create_dir_all(&dir).unwrap();
    graphmind_ok(env, env.home.path(), &["register", dir.to_str().unwrap()]);
    dir.canonicalize().unwrap()
}

/// The binary MCP entries point at: the symlink `CliTestEnv` puts in ~/.graphmind/bin.
fn bin(env: &CliTestEnv) -> String {
    env.home.path().join(".graphmind/bin/graphmind").to_string_lossy().into_owned()
}

fn entry(env: &CliTestEnv) -> Value {
    json!({ "type": "stdio", "command": bin(env), "args": ["mcp"] })
}

fn claude_json(env: &CliTestEnv) -> PathBuf {
    env.home.path().join(".claude.json")
}

fn read(path: &Path) -> Value {
    serde_json::from_str(&fs::read_to_string(path).unwrap()).unwrap()
}

fn project_entry<'a>(config: &'a Value, project: &Path) -> &'a Value {
    &config["projects"][project.to_str().unwrap()]["mcpServers"]["graphmind"]
}

/// A fake `claude` in `<home>/bin`: logs `<cwd> <args>` to `<home>/claude-calls.log`, runs `body`.
#[cfg(unix)]
fn fake_claude(env: &CliTestEnv, body: &str) {
    use std::os::unix::fs::PermissionsExt;
    let dir = env.home.path().join("bin");
    fs::create_dir_all(&dir).unwrap();
    let script = dir.join("claude");
    fs::write(&script, format!("#!/bin/sh\necho \"$(pwd -P) $*\" >> \"$HOME/claude-calls.log\"\n{body}\n")).unwrap();
    fs::set_permissions(&script, fs::Permissions::from_mode(0o755)).unwrap();
}

fn setup_version(env: &CliTestEnv) -> u64 {
    read(&env.home.path().join(".graphmind/config.json"))["setup_version"].as_u64().unwrap_or(0)
}

// ---------------------------------------------------------------------------
// init
// ---------------------------------------------------------------------------

#[test]
fn init_registers_the_project_for_claude_code() {
    let env = CliTestEnv::new();
    let project = repo(&env, "app");
    fs::write(claude_json(&env), r#"{"numStartups": 7, "projects": {"/other": {"allowedTools": []}}}"#).unwrap();

    graphmind_ok(&env, &project, &["init", "--skip-build"]);

    let config = read(&claude_json(&env));
    assert_eq!(*project_entry(&config, &project), entry(&env), "{config}");
    assert!(config["mcpServers"].is_null(), "nothing at user scope: {config}");
    assert_eq!(config["numStartups"], 7);
    assert_eq!(config["projects"]["/other"], json!({ "allowedTools": [] }));

    // Again: already current, nothing rewritten.
    let before = fs::read_to_string(claude_json(&env)).unwrap();
    graphmind_ok(&env, &project, &["init", "--skip-build"]);
    assert_eq!(fs::read_to_string(claude_json(&env)).unwrap(), before);
    assert_eq!(backups_of(&claude_json(&env)), 1, "one backup, for the first write");
}

#[test]
fn init_writes_where_claude_config_dir_points() {
    let env = CliTestEnv::new();
    let project = repo(&env, "app");
    let config_dir = env.home.path().join("claude-config");
    fs::create_dir_all(&config_dir).unwrap();

    let out = graphmind(&env, &project, &["init", "--skip-build"], &[("CLAUDE_CONFIG_DIR", &config_dir)]);
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));

    let config = read(&config_dir.join(".claude.json"));
    assert_eq!(*project_entry(&config, &project), entry(&env), "{config}");
    assert!(!claude_json(&env).exists(), "no entry in ~/.claude.json");
}

#[cfg(unix)]
#[test]
fn init_goes_through_the_claude_cli_in_the_project() {
    let env = CliTestEnv::new();
    let project = repo(&env, "app");
    // Writes .claude.json the way `claude mcp add -s local` does (keyed by cwd, empty `env`).
    fake_claude(
        &env,
        r#"case "$2" in
  add) printf '{"projects":{"%s":{"mcpServers":{"graphmind":{"type":"stdio","command":"%s","args":["mcp"],"env":{}}}}}}' "$(pwd -P)" "$7" > "$HOME/.claude.json" ;;
esac"#,
    );

    graphmind_ok(&env, &project, &["init", "--skip-build"]);

    let log = fs::read_to_string(env.home.path().join("claude-calls.log")).unwrap();
    assert_eq!(log, format!("{} mcp add -s local graphmind -- {} mcp\n", project.display(), bin(&env)));
    assert_eq!(backups_of(&claude_json(&env)), 0, "the CLI wrote the file, not graphmind");
    assert_eq!(project_entry(&read(&claude_json(&env)), &project)["command"], bin(&env));
}

#[cfg(unix)]
#[test]
fn init_writes_the_file_when_the_claude_cli_fails() {
    let env = CliTestEnv::new();
    let project = repo(&env, "app");
    fake_claude(&env, "echo 'config locked' >&2\nexit 1");
    fs::write(claude_json(&env), r#"{"numStartups": 7}"#).unwrap();

    let out = graphmind_ok(&env, &project, &["init", "--skip-build"]);

    assert!(out.contains("config locked"), "the reason is reported: {out}");
    assert!(fs::read_to_string(env.home.path().join("claude-calls.log")).unwrap().contains("mcp add -s local"));
    let config = read(&claude_json(&env));
    assert_eq!(*project_entry(&config, &project), entry(&env), "{config}");
    assert_eq!(config["numStartups"], 7);
}

// ---------------------------------------------------------------------------
// register / unregister
// ---------------------------------------------------------------------------

#[test]
fn register_adds_the_project_entry_and_unregister_removes_it() {
    let env = CliTestEnv::new();
    let project = repo(&env, "app");
    let key = project.to_str().unwrap();
    fs::write(claude_json(&env), r#"{"numStartups": 7}"#).unwrap();

    graphmind_ok(&env, env.home.path(), &["register", key, "--slug", "app"]);

    let mut config = read(&claude_json(&env));
    assert_eq!(*project_entry(&config, &project), entry(&env), "{config}");
    assert_eq!(config["numStartups"], 7);

    // Another server in that project stays when the project is unregistered.
    config["projects"][key]["mcpServers"]["other"] = json!({ "command": "other" });
    fs::write(claude_json(&env), config.to_string()).unwrap();
    graphmind_ok(&env, env.home.path(), &["unregister", "app"]);

    let config = read(&claude_json(&env));
    assert!(project_entry(&config, &project).is_null(), "{config}");
    assert_eq!(config["projects"][key]["mcpServers"]["other"]["command"], "other");
    assert_eq!(config["numStartups"], 7);
}

/// `unregister <typo>` inside a registered project fails and removes nothing: not the project,
/// not its entry.
#[test]
fn unregister_of_an_unknown_slug_keeps_the_project_and_its_entry() {
    let env = CliTestEnv::new();
    let project = repo(&env, "app");
    graphmind_ok(&env, env.home.path(), &["register", project.to_str().unwrap(), "--slug", "app"]);
    let before = fs::read_to_string(claude_json(&env)).unwrap();

    let out = graphmind(&env, &project, &["unregister", "nosuch"], &[]);

    assert!(!out.status.success(), "{}", String::from_utf8_lossy(&out.stdout));
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("Project 'nosuch' is not registered (see `graphmind list`)"), "{stderr}");
    assert_eq!(fs::read_to_string(claude_json(&env)).unwrap(), before);
    assert!(graphmind_ok(&env, env.home.path(), &["list"]).contains("app"));
}

#[test]
fn an_unparseable_claude_json_does_not_block_registration() {
    let env = CliTestEnv::new();
    let project = repo(&env, "app");
    let broken = r#"{"projects": {"#;
    fs::write(claude_json(&env), broken).unwrap();

    let out = graphmind_ok(&env, env.home.path(), &["register", project.to_str().unwrap(), "--slug", "app"]);

    assert!(out.contains("is not valid JSON"), "the reason is reported: {out}");
    assert_eq!(fs::read_to_string(claude_json(&env)).unwrap(), broken);
    let registry = read(&env.home.path().join(".graphmind/config.json"));
    assert_eq!(registry["projects"]["app"]["path"], project.to_str().unwrap(), "registered anyway: {registry}");
}

// ---------------------------------------------------------------------------
// setup
// ---------------------------------------------------------------------------

#[test]
fn setup_refreshes_a_stale_project_entry() {
    let env = CliTestEnv::new();
    let project = registered_folder(&env, "app");
    let key = project.to_str().unwrap();
    fs::write(
        claude_json(&env),
        json!({
            "numStartups": 3,
            "projects": { key: {
                "allowedTools": [],
                "mcpServers": {
                    "graphmind": { "command": "/moved/graphmind", "args": ["mcp"], "env": { "PATH": "/usr/bin" } },
                    "other": { "command": "other" }
                }
            } }
        })
        .to_string(),
    )
    .unwrap();

    graphmind_ok(&env, env.home.path(), &["setup"]);

    let config = read(&claude_json(&env));
    assert_eq!(*project_entry(&config, &project), entry(&env), "{config}");
    assert_eq!(config["projects"][key]["mcpServers"]["other"]["command"], "other");
    assert_eq!(config["projects"][key]["allowedTools"], json!([]));
    assert_eq!(config["numStartups"], 3);
    assert!(config["mcpServers"].is_null(), "nothing at user scope: {config}");
    assert_eq!(setup_version(&env), u64::from(graphmind_config::SETUP_VERSION));
}

#[test]
fn setup_leaves_current_entries_byte_identical() {
    let env = CliTestEnv::new();
    let project = registered_folder(&env, "app");
    let content = format!(
        "{{\"projects\":{{\"{}\":{{\"mcpServers\":{{\"graphmind\":{{\"type\":\"stdio\",\"command\":\"{}\",\"args\":[\"mcp\"],\"env\":{{}}}}}}}}}}}}",
        project.display(),
        bin(&env)
    );
    fs::write(claude_json(&env), &content).unwrap();

    graphmind_ok(&env, env.home.path(), &["setup"]);

    assert_eq!(fs::read_to_string(claude_json(&env)).unwrap(), content);
    assert_eq!(backups_of(&claude_json(&env)), 0, "no backup when nothing changes");
    assert_eq!(setup_version(&env), u64::from(graphmind_config::SETUP_VERSION));
}

#[test]
fn setup_replaces_the_user_scope_entry_with_project_entries() {
    let env = CliTestEnv::new();
    let project = registered_folder(&env, "app");
    fs::write(
        claude_json(&env),
        json!({
            "numStartups": 3,
            "mcpServers": {
                "graphmind": { "type": "stdio", "command": "/opt/old/.graphmind/bin/graphmind", "args": ["mcp"], "env": {} },
                "other": { "command": "other" }
            }
        })
        .to_string(),
    )
    .unwrap();

    let out = graphmind_ok(&env, env.home.path(), &["setup"]);

    let config = read(&claude_json(&env));
    assert!(config["mcpServers"]["graphmind"].is_null(), "user-scope entry removed: {config}");
    assert_eq!(config["mcpServers"]["other"]["command"], "other", "other user-scope servers kept");
    assert_eq!(*project_entry(&config, &project), entry(&env), "{config}");
    assert_eq!(config["numStartups"], 3);
    assert!(out.contains("removed the user-scope entry"), "reported: {out}");
}

#[test]
fn setup_keeps_a_user_scope_server_it_did_not_write() {
    let env = CliTestEnv::new();
    let mine = json!({ "mcpServers": { "graphmind": { "command": "/usr/local/bin/my-graphmind-wrapper", "args": [] } } });
    fs::write(claude_json(&env), mine.to_string()).unwrap();

    graphmind_ok(&env, env.home.path(), &["setup"]);

    assert_eq!(read(&claude_json(&env)), mine);
}

#[test]
fn setup_leaves_an_unparseable_claude_json_alone_and_retries() {
    let env = CliTestEnv::new();
    registered_folder(&env, "app");
    let broken = r#"{"numStartups": 3, "projects": {"#; // Claude Code caught mid-write
    fs::write(claude_json(&env), broken).unwrap();

    let out = graphmind_ok(&env, env.home.path(), &["setup"]);

    assert_eq!(fs::read_to_string(claude_json(&env)).unwrap(), broken);
    assert_eq!(backups_of(&claude_json(&env)), 0);
    assert!(out.contains("is not valid JSON"), "the reason is reported: {out}");
    assert_eq!(setup_version(&env), 0, "auto-setup must retry");

    // Readable again: nothing to refresh, so nothing written, and setup is complete. Setup adds no
    // entry for a project that has none (`init` and `register` do).
    fs::write(claude_json(&env), r#"{"numStartups": 3}"#).unwrap();
    graphmind_ok(&env, env.home.path(), &["setup"]);
    assert_eq!(fs::read_to_string(claude_json(&env)).unwrap(), r#"{"numStartups": 3}"#);
    assert_eq!(setup_version(&env), u64::from(graphmind_config::SETUP_VERSION));
}

// ---------------------------------------------------------------------------
// uninstall all
// ---------------------------------------------------------------------------

#[test]
fn uninstall_all_removes_graphmind_entries_only() {
    let env = CliTestEnv::new();
    let project = registered_folder(&env, "app");
    let key = project.to_str().unwrap();
    fs::write(
        claude_json(&env),
        json!({
            "numStartups": 3,
            "mcpServers": { "graphmind": { "command": bin(&env), "args": ["mcp"] }, "other": { "command": "other" } },
            "projects": {
                key: { "mcpServers": { "graphmind": entry(&env) } },
                "/gone/project": { "allowedTools": [], "mcpServers": { "graphmind": { "command": "/old/graphmind" }, "other": { "command": "other" } } },
                "/hand/made": { "mcpServers": { "graphmind": { "command": "/opt/my-wrapper" } } }
            }
        })
        .to_string(),
    )
    .unwrap();

    graphmind_ok(&env, env.home.path(), &["uninstall", "all"]);

    let config = read(&claude_json(&env));
    assert!(config["mcpServers"]["graphmind"].is_null(), "{config}");
    assert_eq!(config["mcpServers"]["other"]["command"], "other");
    assert!(project_entry(&config, &project).is_null(), "{config}");
    assert!(config["projects"]["/gone/project"]["mcpServers"]["graphmind"].is_null(), "unregistered project too: {config}");
    assert_eq!(config["projects"]["/gone/project"]["mcpServers"]["other"]["command"], "other");
    assert_eq!(config["projects"]["/gone/project"]["allowedTools"], json!([]));
    assert_eq!(config["projects"]["/hand/made"]["mcpServers"]["graphmind"]["command"], "/opt/my-wrapper");
    assert_eq!(config["numStartups"], 3);
}

/// The main worktree of the repo containing `dir`, as `git worktree list` reports it: the path
/// Claude Code files the repo's local-scope servers under.
fn main_worktree(dir: &Path) -> PathBuf {
    let out = Command::new("git").arg("-C").arg(dir).args(["worktree", "list", "--porcelain"]).output().unwrap();
    let list = String::from_utf8_lossy(&out.stdout).to_string();
    let first = list.lines().next().and_then(|l| l.strip_prefix("worktree ")).expect("a main worktree");
    PathBuf::from(first).canonicalize().unwrap()
}

/// The projects of .claude.json that hold a graphmind entry.
fn graphmind_keys(config: &Value) -> Vec<String> {
    let projects = config["projects"].as_object().cloned().unwrap_or_default();
    projects.into_iter().filter(|(_, p)| !p["mcpServers"]["graphmind"].is_null()).map(|(key, _)| key).collect()
}

fn key(path: &Path) -> String {
    path.to_str().unwrap().to_string()
}

#[test]
fn init_in_a_worktree_or_a_subfolder_uses_the_main_checkout_entry() {
    let env = CliTestEnv::new();
    let app = repo(&env, "app");
    fs::create_dir_all(app.join("packages/api")).unwrap();
    let worktree = env.home.path().join("app-wt");
    git(&app, &["worktree", "add", "-q", "--detach", worktree.to_str().unwrap()]);
    let worktree = worktree.canonicalize().unwrap();
    assert_eq!(main_worktree(&worktree), app);
    assert_eq!(main_worktree(&app.join("packages/api")), app, "the key Claude Code reads in the subfolder");

    let out = graphmind_ok(&env, &worktree, &["init", "--skip-build"]);

    let config = read(&claude_json(&env));
    assert_eq!(graphmind_keys(&config), vec![key(&app)], "{config}");
    assert_eq!(*project_entry(&config, &app), entry(&env));
    assert!(out.contains(&format!("Claude Code entry under {}", app.display())), "the key is reported: {out}");

    graphmind_ok(&env, &app.join("packages/api"), &["init", "--skip-build"]);
    assert_eq!(graphmind_keys(&read(&claude_json(&env))), vec![key(&app)]);
    assert_eq!(backups_of(&claude_json(&env)), 0, "written once, on a missing file; then confirmed");
}

#[test]
fn setup_migration_writes_one_entry_per_repository_and_backs_up_the_original() {
    let env = CliTestEnv::new();
    let app = repo(&env, "app");
    let worktree = env.home.path().join("app-wt");
    git(&app, &["worktree", "add", "-q", "--detach", worktree.to_str().unwrap()]);
    let mono = repo(&env, "mono");
    fs::create_dir_all(mono.join("packages/api")).unwrap();
    for dir in [app.clone(), worktree.clone(), mono.join("packages/api")] {
        graphmind_ok(&env, env.home.path(), &["register", dir.to_str().unwrap()]);
    }
    let plain = registered_folder(&env, "plain");
    // Registering wrote entries (and backups): start over from a user-scope entry only.
    let backups = |env: &CliTestEnv| -> Vec<PathBuf> {
        fs::read_dir(env.home.path())
            .unwrap()
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.to_string_lossy().contains(".claude.json.") && p.to_string_lossy().ends_with(".bak"))
            .collect()
    };
    backups(&env).into_iter().for_each(|b| fs::remove_file(b).unwrap());
    let original = json!({
        "numStartups": 3,
        "mcpServers": { "graphmind": { "type": "stdio", "command": bin(&env), "args": ["mcp"], "env": {} }, "other": { "command": "other" } }
    })
    .to_string();
    fs::write(claude_json(&env), &original).unwrap();

    graphmind_ok(&env, env.home.path(), &["setup"]);

    let config = read(&claude_json(&env));
    let mut expected = vec![key(&app), key(&mono), key(&plain)];
    expected.sort();
    assert_eq!(graphmind_keys(&config), expected, "no worktree or subfolder key: {config}");
    assert_eq!(config["projects"].as_object().unwrap().len(), 3, "{config}");
    assert!(config["mcpServers"]["graphmind"].is_null(), "user-scope entry removed: {config}");
    assert_eq!(config["mcpServers"]["other"]["command"], "other");
    assert_eq!(setup_version(&env), u64::from(graphmind_config::SETUP_VERSION));

    let backups = backups(&env);
    assert_eq!(backups.len(), 1, "one write: {backups:?}");
    assert_eq!(fs::read_to_string(&backups[0]).unwrap(), original, "the backup holds the file as it was");
}

#[test]
fn init_and_register_keep_a_hand_made_entry() {
    let env = CliTestEnv::new();
    let app = repo(&env, "app");
    let mine = json!({ "projects": { key(&app): { "mcpServers": { "graphmind": { "command": "/opt/my-wrapper.sh", "args": [] } } } } })
        .to_string();
    fs::write(claude_json(&env), &mine).unwrap();

    let out = graphmind_ok(&env, &app, &["init", "--skip-build"]);
    assert!(out.contains("left alone: hand-made graphmind entry"), "reported: {out}");
    let out = graphmind_ok(&env, env.home.path(), &["register", key(&app).as_str(), "--slug", "app"]);
    assert!(out.contains("left alone: hand-made graphmind entry"), "reported: {out}");
    let out = graphmind_ok(&env, env.home.path(), &["unregister", "app"]);
    assert!(out.contains("left alone: hand-made graphmind entry"), "reported: {out}");

    assert_eq!(fs::read_to_string(claude_json(&env)).unwrap(), mine);
    assert_eq!(backups_of(&claude_json(&env)), 0);
}

#[test]
fn unregister_reports_why_an_unparseable_claude_json_is_left_alone() {
    let env = CliTestEnv::new();
    let app = repo(&env, "app");
    graphmind_ok(&env, env.home.path(), &["register", key(&app).as_str(), "--slug", "app"]);
    let broken = r#"{"projects": {"#;
    fs::write(claude_json(&env), broken).unwrap();

    let out = graphmind_ok(&env, env.home.path(), &["unregister", "app"]);

    assert!(out.contains("is not valid JSON"), "the reason is reported: {out}");
    assert_eq!(fs::read_to_string(claude_json(&env)).unwrap(), broken);
}

#[test]
fn unregister_keeps_an_entry_another_registered_project_uses() {
    let env = CliTestEnv::new();
    let app = repo(&env, "app");
    graphmind_ok(&env, env.home.path(), &["register", key(&app).as_str(), "--slug", "app"]);
    fs::create_dir_all(app.join("packages/api")).unwrap();
    graphmind_ok(&env, env.home.path(), &["register", app.join("packages/api").to_str().unwrap(), "--slug", "api"]);

    let out = graphmind_ok(&env, env.home.path(), &["unregister", "api"]);
    assert!(out.contains("another registered project uses this entry"), "{out}");
    assert_eq!(*project_entry(&read(&claude_json(&env)), &app), entry(&env));

    graphmind_ok(&env, env.home.path(), &["unregister", "app"]);
    assert!(project_entry(&read(&claude_json(&env)), &app).is_null(), "the last project takes it");
}

#[test]
fn setup_with_no_registered_project_removes_the_user_scope_entry_and_says_so() {
    let env = CliTestEnv::new();
    let user_scope = json!({ "mcpServers": { "graphmind": { "type": "stdio", "command": bin(&env), "args": ["mcp"] } } });
    fs::write(claude_json(&env), user_scope.to_string()).unwrap();

    let out = graphmind_ok(&env, env.home.path(), &["setup"]);

    assert!(read(&claude_json(&env))["mcpServers"]["graphmind"].is_null());
    assert!(out.contains("no project registered: run graphmind init in a project"), "{out}");
    assert!(
        !out.lines().any(|l| l.contains("configured for") && l.contains("Claude Code")),
        "Claude Code is not reported configured: {out}"
    );
}

#[test]
fn setup_migrates_where_claude_config_dir_points() {
    let env = CliTestEnv::new();
    let config_dir = env.home.path().join("claude-config");
    fs::create_dir_all(&config_dir).unwrap();
    let app = env.home.path().join("app");
    fs::create_dir_all(&app).unwrap();
    let with_dir = [("CLAUDE_CONFIG_DIR", config_dir.as_path())];
    assert!(graphmind(&env, env.home.path(), &["register", app.to_str().unwrap()], &with_dir).status.success());
    let user_scope = json!({ "mcpServers": { "graphmind": { "command": "/old/.graphmind/bin/graphmind", "args": ["mcp"] } } });
    fs::write(config_dir.join(".claude.json"), user_scope.to_string()).unwrap();

    let out = graphmind(&env, env.home.path(), &["setup"], &with_dir);
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));

    let config = read(&config_dir.join(".claude.json"));
    assert!(config["mcpServers"]["graphmind"].is_null(), "{config}");
    assert_eq!(*project_entry(&config, &app.canonicalize().unwrap()), entry(&env), "{config}");
    assert!(!claude_json(&env).exists(), "~/.claude.json untouched");
}

#[cfg(unix)]
#[test]
fn setup_removes_the_user_scope_entry_through_the_claude_cli() {
    let env = CliTestEnv::new();
    let app = registered_folder(&env, "app");
    let other = json!({ "command": "other" });
    fs::write(
        claude_json(&env),
        json!({ "mcpServers": { "graphmind": { "type": "stdio", "command": bin(&env), "args": ["mcp"] }, "other": other }, "projects": {} })
            .to_string(),
    )
    .unwrap();
    // What `claude mcp remove -s user graphmind` leaves: the file without the user-scope entry.
    let after = json!({ "mcpServers": { "other": other }, "projects": { key(&app): { "mcpServers": { "graphmind": entry(&env) } } } });
    fake_claude(&env, &format!("case \"$2\" in\n  remove) printf '%s' '{after}' > \"$HOME/.claude.json\" ;;\nesac"));

    let out = graphmind_ok(&env, env.home.path(), &["setup"]);

    let log = fs::read_to_string(env.home.path().join("claude-calls.log")).unwrap();
    let home = env.home.path().canonicalize().unwrap();
    assert_eq!(log, format!("{} mcp remove -s user graphmind\n", home.display()), "{out}");
    assert_eq!(read(&claude_json(&env)), after);
    assert!(out.contains("removed the user-scope entry"), "{out}");
    assert_eq!(setup_version(&env), u64::from(graphmind_config::SETUP_VERSION));
}

#[test]
fn setup_moves_worktree_and_subfolder_entries_to_the_main_checkout() {
    let env = CliTestEnv::new();
    let app = repo(&env, "app");
    let worktree = env.home.path().join("app-wt");
    git(&app, &["worktree", "add", "-q", "--detach", worktree.to_str().unwrap()]);
    let worktree = worktree.canonicalize().unwrap();
    let mono = repo(&env, "mono");
    let api = mono.join("packages/api");
    fs::create_dir_all(&api).unwrap();
    for dir in [&app, &worktree, &api] {
        graphmind_ok(&env, env.home.path(), &["register", dir.to_str().unwrap()]);
    }
    // An entry under each registered folder: the main checkout's current, the worktree's and the
    // subfolder's stale. Claude Code never reads the worktree's or the subfolder's.
    let stale = json!({ "command": "/old/graphmind", "args": ["mcp"], "env": { "PATH": "/usr/bin" } });
    let mine = json!({ "command": "mine" });
    fs::write(
        claude_json(&env),
        json!({ "projects": {
            key(&app): { "mcpServers": { "graphmind": entry(&env) } },
            key(&worktree): { "allowedTools": [], "mcpServers": { "graphmind": stale, "mine": mine } },
            key(&api): { "allowedTools": ["Bash"], "mcpServers": { "graphmind": stale } }
        } })
        .to_string(),
    )
    .unwrap();

    let out = graphmind_ok(&env, env.home.path(), &["setup"]);

    let config = read(&claude_json(&env));
    let mut expected = vec![key(&app), key(&mono)];
    expected.sort();
    assert_eq!(graphmind_keys(&config), expected, "{config}");
    assert_eq!(*project_entry(&config, &mono), entry(&env), "the subfolder's entry moved to its repository");
    assert_eq!(config["projects"][key(&worktree)]["mcpServers"]["mine"], mine, "other servers stay");
    assert_eq!(config["projects"][key(&api)], json!({ "allowedTools": ["Bash"] }), "no empty mcpServers left");
    assert!(out.contains(&format!("removed the entry under {}", api.display())), "{out}");
    assert_eq!(setup_version(&env), u64::from(graphmind_config::SETUP_VERSION));
}

#[test]
fn unregister_of_a_deleted_subfolder_removes_its_repository_entry() {
    let env = CliTestEnv::new();
    let app = repo(&env, "app");
    let api = app.join("packages/api");
    fs::create_dir_all(&api).unwrap();
    graphmind_ok(&env, env.home.path(), &["register", api.to_str().unwrap(), "--slug", "api"]);
    assert_eq!(*project_entry(&read(&claude_json(&env)), &app), entry(&env));
    fs::remove_dir_all(app.join("packages")).unwrap();

    graphmind_ok(&env, env.home.path(), &["unregister", "api"]);

    let config = read(&claude_json(&env));
    assert!(graphmind_keys(&config).is_empty(), "the repository's entry goes with its last project: {config}");
}

#[cfg(unix)]
#[test]
fn setup_backs_up_once_when_the_claude_cli_fails_to_remove_the_user_scope_entry() {
    let env = CliTestEnv::new();
    let app = registered_folder(&env, "app");
    let original = json!({ "mcpServers": { "graphmind": { "type": "stdio", "command": bin(&env), "args": ["mcp"] } } }).to_string();
    fs::write(claude_json(&env), &original).unwrap();
    fs::remove_file(env.home.path().join("claude-calls.log")).ok();
    for backup in fs::read_dir(env.home.path()).unwrap().flatten().map(|e| e.path()) {
        if backup.to_string_lossy().contains(".claude.json.") {
            fs::remove_file(backup).unwrap();
        }
    }
    fake_claude(&env, "echo 'remove failed' >&2\nexit 1");

    let out = graphmind_ok(&env, env.home.path(), &["setup"]);

    let config = read(&claude_json(&env));
    assert!(config["mcpServers"]["graphmind"].is_null(), "removed by a direct write: {config}");
    assert_eq!(*project_entry(&config, &app), entry(&env), "{config}");
    assert!(out.contains("`claude mcp remove` failed (remove failed)"), "{out}");
    let backups: Vec<PathBuf> = fs::read_dir(env.home.path())
        .unwrap()
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.to_string_lossy().contains(".claude.json.") && p.to_string_lossy().ends_with(".bak"))
        .collect();
    assert_eq!(backups.len(), 1, "one backup per run: {backups:?}");
    assert_eq!(fs::read_to_string(&backups[0]).unwrap(), original, "it holds the file as it was");
}

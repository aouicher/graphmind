//! `graphmind install hook-git` writes only into the repo's own git dir, and builds of one
//! project run one at a time.

mod common;
use common::CliTestEnv;
use std::path::Path;
use std::process::Command;

fn git(dir: &Path, args: &[&str]) {
    let out = Command::new("git").arg("-C").arg(dir).args(args).output().unwrap();
    assert!(out.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&out.stderr));
}

fn new_repo(env: &CliTestEnv, name: &str) -> std::path::PathBuf {
    let repo = env.home.path().join(name);
    std::fs::create_dir_all(repo.join("src")).unwrap();
    std::fs::write(repo.join("src/a.ts"), "export function a() {}\n").unwrap();
    git(&repo, &["init", "-q"]);
    git(&repo, &["-c", "user.email=t@t", "-c", "user.name=t", "add", "."]);
    git(&repo, &["-c", "user.email=t@t", "-c", "user.name=t", "commit", "-q", "-m", "init"]);
    repo
}

const HOOK_NAMES: [&str; 5] = ["post-commit", "pre-push", "post-checkout", "post-merge", "post-rewrite"];

#[test]
fn installs_all_hooks_into_the_git_dir() {
    let env = CliTestEnv::new();
    let repo = new_repo(&env, "plain");
    env.run_ok(&["register", repo.to_str().unwrap(), "--slug", "plain"]);
    env.run_ok(&["install", "hook-git", "plain"]);
    for name in HOOK_NAMES {
        let body = std::fs::read_to_string(repo.join(".git/hooks").join(name)).unwrap_or_default();
        assert!(body.contains("graphmind"), "{name} not installed");
    }
    env.run_ok(&["uninstall", "hook-git", "plain"]);
    assert!(!repo.join(".git/hooks/post-merge").exists());
}

#[test]
fn husky_repo_gets_instructions_not_edits() {
    let env = CliTestEnv::new();
    let repo = new_repo(&env, "husky");
    std::fs::create_dir_all(repo.join(".husky/_")).unwrap();
    std::fs::write(repo.join(".husky/_/post-commit"), "#!/bin/sh\n. \"$(dirname \"$0\")/h\"\n").unwrap();
    git(&repo, &["config", "core.hooksPath", ".husky/_"]);
    env.run_ok(&["register", repo.to_str().unwrap(), "--slug", "husky"]);

    let out = env.run_ok(&["install", "hook-git", "husky"]);
    assert!(out.contains("husky"), "{out}");
    assert!(out.contains(".husky/post-commit"), "points at the editable husky script: {out}");
    let wrapper = std::fs::read_to_string(repo.join(".husky/_/post-commit")).unwrap();
    assert!(!wrapper.contains("graphmind"), "husky's generated wrapper must not be edited");
    assert!(!repo.join(".husky/post-commit").exists(), "tracked husky scripts must not be created");
}

#[test]
fn shared_hooks_folder_is_left_alone() {
    let env = CliTestEnv::new();
    let repo = new_repo(&env, "shared");
    let global = env.home.path().join("global-hooks");
    std::fs::create_dir_all(&global).unwrap();
    git(&repo, &["config", "core.hooksPath", global.to_str().unwrap()]);
    env.run_ok(&["register", repo.to_str().unwrap(), "--slug", "shared"]);

    let out = env.run_ok(&["install", "hook-git", "shared"]);
    assert!(out.contains("shared folder"), "{out}");
    assert_eq!(std::fs::read_dir(&global).unwrap().count(), 0, "global hooks folder was modified");
}

#[test]
fn second_build_waits_for_the_first() {
    use fs4::FileExt;
    let env = CliTestEnv::new();
    let repo = new_repo(&env, "locked");
    env.run_ok(&["register", repo.to_str().unwrap(), "--slug", "locked"]);
    env.run_ok(&["build", "locked"]);

    // Hold the project's build lock as a running build would.
    let lock_path = env.home.path().join(".graphmind/graphs/locked/build.lock");
    let lock = std::fs::OpenOptions::new().create(true).write(true).truncate(false).open(&lock_path).unwrap();
    FileExt::lock(&lock).unwrap();

    let mut child = Command::new(&env.binary)
        .args(["build", "locked"])
        .env("HOME", env.home.path())
        .env_remove("CLAUDE_CONFIG_DIR")
        .env("GRAPHMIND_SKIP_NOTICES", "1")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    std::thread::sleep(std::time::Duration::from_millis(800));
    assert!(child.try_wait().unwrap().is_none(), "build should wait while another build holds the lock");

    FileExt::unlock(&lock).unwrap();
    let out = child.wait_with_output().unwrap();
    assert!(out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("Waiting for another build"));
}

#[test]
fn worktree_add_registers_and_seeds_the_new_worktree() {
    let env = CliTestEnv::new();
    let repo = new_repo(&env, "app");
    env.run_ok(&["register", repo.to_str().unwrap(), "--slug", "app"]);
    env.run_ok(&["install", "hook-git", "app"]);
    env.run_ok(&["build", "app"]);

    // post-checkout runs `graphmind build` in the background from the new checkout.
    let wt = env.home.path().join("app-feature");
    let out = Command::new("git")
        .arg("-C")
        .arg(&repo)
        .args(["worktree", "add", "-q", "-b", "feature", wt.to_str().unwrap()])
        .env("HOME", env.home.path())
        .env_remove("CLAUDE_CONFIG_DIR")
        .env("GRAPHMIND_SKIP_NOTICES", "1")
        .output()
        .unwrap();
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));

    let meta = env.home.path().join(".graphmind/graphs/app-feature/meta.json");
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
    while !meta.exists() && std::time::Instant::now() < deadline {
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
    let meta: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&meta).expect("worktree was not built")).unwrap();
    assert_eq!(
        meta["build_result"]["files_processed"], 0,
        "seeded from app's graph, nothing to re-parse: {meta}"
    );
    let list = env.run_ok(&["list"]);
    assert!(list.contains("worktree of app"), "got: {list}");
}

#[test]
fn setup_completes_hooks_installed_by_an_older_version() {
    let env = CliTestEnv::new();
    let old = new_repo(&env, "old");
    let never = new_repo(&env, "never");
    env.run_ok(&["register", old.to_str().unwrap(), "--slug", "old"]);
    env.run_ok(&["register", never.to_str().unwrap(), "--slug", "never"]);

    // What graphmind <= 0.2.215 installed: post-commit and pre-push only.
    let hooks = old.join(".git/hooks");
    std::fs::write(hooks.join("post-commit"), "#!/bin/sh\n# graphmind: auto-rebuild\ngraphmind build 2>/dev/null &\n").unwrap();
    std::fs::write(hooks.join("pre-push"), "#!/bin/sh\n# graphmind: diff-impact\ngraphmind diff-impact 2>/dev/null\n").unwrap();

    env.run_ok(&["setup"]);

    for name in ["post-checkout", "post-merge", "post-rewrite"] {
        let body = std::fs::read_to_string(hooks.join(name)).unwrap_or_default();
        assert!(body.contains("graphmind"), "{name} should be added");
    }
    for name in HOOK_NAMES {
        assert!(!never.join(".git/hooks").join(name).exists(), "{name} must not be installed unasked");
    }
}

#[test]
fn auto_setup_after_an_upgrade_finishes_after_the_command_exits() {
    let env = CliTestEnv::new();
    let repo = new_repo(&env, "upgraded");
    env.run_ok(&["register", repo.to_str().unwrap(), "--slug", "upgraded"]);
    let hooks = repo.join(".git/hooks");
    std::fs::write(hooks.join("post-commit"), "#!/bin/sh\n# graphmind: auto-rebuild\ngraphmind build 2>/dev/null &\n").unwrap();

    // The binary was just replaced by a newer one: its setup version is ahead of the config's.
    let config_path = env.home.path().join(".graphmind/config.json");
    let mut cfg: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&config_path).unwrap()).unwrap();
    cfg["setup_version"] = serde_json::json!(3);
    std::fs::write(&config_path, cfg.to_string()).unwrap();
    // The previous version's own auto-setup ran a moment ago.
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs();
    std::fs::write(env.home.path().join(".graphmind/auto-setup.stamp"), now.to_string()).unwrap();

    // Any quick command, notices enabled (as for a real user).
    let out = Command::new(&env.binary)
        .arg("list")
        .env("HOME", env.home.path())
        .env_remove("CLAUDE_CONFIG_DIR")
        .env("GRAPHMIND_NO_AUTO_BUILD", "1")
        .output()
        .unwrap();
    assert!(out.status.success());
    assert!(
        !String::from_utf8_lossy(&out.stdout).contains("Setup complete"),
        "setup output must not leak into the command's stdout"
    );

    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(60);
    while !hooks.join("post-checkout").exists() && std::time::Instant::now() < deadline {
        std::thread::sleep(std::time::Duration::from_millis(200));
    }
    assert!(hooks.join("post-checkout").exists(), "setup should complete on its own");
}

/// `graphmind init <PATH>` hooks and builds `<PATH>`, whatever the current directory: outside any
/// repo (#183), and inside another registered repo, which it must leave alone.
#[test]
fn init_with_a_path_targets_that_path() {
    let env = CliTestEnv::new();
    let target = new_repo(&env, "target");
    let other = new_repo(&env, "other");
    env.run_ok(&["register", other.to_str().unwrap(), "--slug", "other"]);

    let out = env.run_ok_in(env.home.path(), &["init", "--skip-build", target.to_str().unwrap()]);
    for name in HOOK_NAMES {
        let body = std::fs::read_to_string(target.join(".git/hooks").join(name)).unwrap_or_default();
        assert!(body.contains("graphmind"), "{name} not installed in target: {out}");
    }

    let target2 = new_repo(&env, "target2");
    env.run_ok_in(&other, &["init", target2.to_str().unwrap()]);
    assert!(target2.join(".git/hooks/post-commit").exists(), "target2 not hooked");
    assert!(!other.join(".git/hooks/post-commit").exists(), "init hooked the cwd's project");
    let outline = env.run_ok(&["outline", "src/a.ts", "--slug", "target2"]);
    assert!(outline.contains("Function a"), "target2 not built: {outline}");
    let other_outline = env.run(&["outline", "src/a.ts", "--slug", "other"]);
    assert!(
        !String::from_utf8_lossy(&other_outline.stdout).contains("Function a"),
        "init built the cwd's project"
    );
}

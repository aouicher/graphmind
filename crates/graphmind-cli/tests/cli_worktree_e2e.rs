//! Git worktree support: every checkout of a repo gets its own graph (its own
//! branch), all of them share one memory, and installs that predate worktree
//! support keep working without any migration.

mod common;
use common::CliTestEnv;
use std::path::{Path, PathBuf};
use std::process::Command;

fn git(dir: &Path, args: &[&str]) {
    let out = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .env("GIT_AUTHOR_NAME", "test")
        .env("GIT_AUTHOR_EMAIL", "test@test.com")
        .env("GIT_COMMITTER_NAME", "test")
        .env("GIT_COMMITTER_EMAIL", "test@test.com")
        .output()
        .expect("failed to run git");
    assert!(
        out.status.success(),
        "git {args:?} failed in {dir:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

/// A git repo at `<home>/<path>` with one Rust file and one commit.
fn init_repo(env: &CliTestEnv, path: &str) -> PathBuf {
    let repo = env.home.path().join(path);
    std::fs::create_dir_all(&repo).unwrap();
    git(&repo, &["init", "-q"]);
    git(&repo, &["config", "commit.gpgsign", "false"]);
    std::fs::write(repo.join("lib.rs"), "pub fn shared_fn() {}\n").unwrap();
    git(&repo, &["add", "."]);
    git(&repo, &["commit", "-q", "-m", "init"]);
    repo
}

/// Main checkout at `repo/` and a second worktree at `wt2/` on branch `feature`.
fn init_repo_with_worktree(env: &CliTestEnv) -> (PathBuf, PathBuf) {
    let repo = init_repo(env, "repo");
    let wt2 = env.home.path().join("wt2");
    git(&repo, &["worktree", "add", "-q", wt2.to_str().unwrap(), "-b", "feature"]);
    (repo, wt2)
}

fn config(env: &CliTestEnv) -> serde_json::Value {
    let raw = std::fs::read_to_string(env.home.path().join(".graphmind/config.json")).unwrap();
    serde_json::from_str(&raw).unwrap()
}

/// The registered project at `path`, from config.json.
fn project_at(env: &CliTestEnv, path: &Path) -> Option<serde_json::Value> {
    let path = path.canonicalize().unwrap();
    config(env)["projects"]
        .as_object()
        .unwrap()
        .values()
        .find(|p| p["path"].as_str() == Some(path.to_str().unwrap()))
        .cloned()
}

fn status_project(out: &str) -> String {
    out.lines()
        .find_map(|l| l.strip_prefix("Project: "))
        .unwrap_or_default()
        .trim()
        .to_string()
}

#[test]
fn unregistered_worktree_auto_links_to_sibling_repo() {
    let env = CliTestEnv::new();
    let (repo, wt2) = init_repo_with_worktree(&env);
    env.run_ok_in(&repo, &["register", ".", "--slug", "main-wt"]);

    let out = env.run_ok_in(&wt2, &["status"]);
    assert_eq!(status_project(&out), "wt2", "status should target the worktree, got: {out}");

    let wt = project_at(&env, &wt2).expect("worktree should be registered");
    assert_eq!(wt["primary"], "main-wt");
    assert_eq!(wt["branch"], "feature");
    let list = env.run_ok(&["list"]);
    assert!(list.contains("worktree of main-wt"), "got: {list}");
}

#[test]
fn nested_claude_code_worktree_gets_its_own_graph() {
    let env = CliTestEnv::new();
    let repo = init_repo(&env, "repo");
    env.run_ok_in(&repo, &["register", ".", "--slug", "main-wt"]);
    env.run_ok_in(&repo, &["build"]);

    // Claude Code's default location: inside the main checkout.
    let wt = repo.join(".claude/worktrees/feat");
    git(&repo, &["worktree", "add", "-q", wt.to_str().unwrap(), "-b", "feat"]);
    std::fs::write(wt.join("feat.rs"), "pub fn only_on_feat() {}\n").unwrap();
    git(&wt, &["add", "."]);
    git(&wt, &["commit", "-q", "-m", "feat"]);

    let out = env.run_ok_in(&wt, &["status"]);
    assert_eq!(
        status_project(&out),
        "feat",
        "a nested worktree must not resolve to the main checkout, got: {out}"
    );

    let build = env.run_ok_in(&wt, &["build"]);
    assert!(build.contains("1 files processed"), "only feat.rs should be parsed: {build}");

    let here = env.run_ok_in(&wt, &["fn", "only_on_feat"]);
    assert!(here.contains("feat.rs"), "worktree graph should have its branch: {here}");
    let there = env.run_in(&repo, &["fn", "only_on_feat"]);
    assert!(
        !String::from_utf8_lossy(&there.stdout).contains("feat.rs"),
        "main checkout's graph must not see the worktree's branch"
    );
}

#[test]
fn linking_seeds_the_worktree_graph_from_its_primary() {
    let env = CliTestEnv::new();
    let (repo, wt2) = init_repo_with_worktree(&env);
    env.run_ok_in(&repo, &["register", ".", "--slug", "main-wt"]);
    env.run_ok_in(&repo, &["build"]);

    env.run_ok_in(&wt2, &["status"]);
    let graphs = env.home.path().join(".graphmind/graphs");
    assert!(graphs.join("wt2/graph.db").exists(), "linking should seed wt2's graph");

    // Queries work before any build of wt2.
    let out = env.run_ok_in(&wt2, &["fn", "shared_fn"]);
    assert!(out.contains("lib.rs"), "got: {out}");
}

#[test]
fn command_in_a_subdirectory_registers_the_worktree_root() {
    let env = CliTestEnv::new();
    let (repo, wt2) = init_repo_with_worktree(&env);
    env.run_ok_in(&repo, &["register", ".", "--slug", "main-wt"]);

    let sub = wt2.join("src/deep");
    std::fs::create_dir_all(&sub).unwrap();
    env.run_ok_in(&sub, &["status"]);

    assert!(project_at(&env, &wt2).is_some(), "the worktree root should be registered");
    assert!(project_at(&env, &sub).is_none(), "not the subdirectory");
}

#[test]
fn memory_is_shared_across_worktrees() {
    let env = CliTestEnv::new();
    let (repo, wt2) = init_repo_with_worktree(&env);

    env.run_ok_in(&repo, &["register", ".", "--slug", "main-wt"]);
    env.run_ok_in(&repo, &["memory", "add", "decision made in main worktree", "--type", "decision", "--in", "main-wt"]);
    env.run_ok_in(&wt2, &["memory", "add", "note added from feature worktree", "--type", "context"]);

    let out = env.run_ok_in(&repo, &["memory", "search", "feature worktree", "--in", "main-wt"]);
    assert!(out.contains("note added from feature worktree"), "got: {out}");
    let out = env.run_ok_in(&wt2, &["memory", "search", "main worktree"]);
    assert!(out.contains("decision made in main worktree"), "got: {out}");

    // One memory file, the primary's own: nothing is renamed.
    let mem_dir = env.home.path().join(".graphmind/memory");
    let files: Vec<_> = std::fs::read_dir(&mem_dir)
        .unwrap()
        .filter_map(|e| e.ok())
        .map(|e| e.file_name().to_string_lossy().to_string())
        .filter(|n| n.ends_with(".jsonl") && n != "global.jsonl")
        .collect();
    assert_eq!(files, vec!["main-wt.jsonl".to_string()]);
}

#[test]
fn install_from_before_worktree_support_links_and_keeps_its_memory() {
    let env = CliTestEnv::new();
    let (repo, wt2) = init_repo_with_worktree(&env);
    env.run_ok_in(&repo, &["register", ".", "--slug", "main-wt"]);
    env.run_ok_in(&repo, &["memory", "add", "legacy convention", "--type", "convention", "--in", "main-wt"]);

    // What an older graphmind wrote: no repo_id, branch or primary.
    let path = env.home.path().join(".graphmind/config.json");
    let mut cfg = config(&env);
    for p in cfg["projects"].as_object_mut().unwrap().values_mut() {
        let p = p.as_object_mut().unwrap();
        p.remove("repo_id");
        p.remove("branch");
        p.remove("primary");
    }
    std::fs::write(&path, serde_json::to_string_pretty(&cfg).unwrap()).unwrap();

    let out = env.run_ok_in(&wt2, &["status"]);
    assert_eq!(status_project(&out), "wt2", "got: {out}");
    let out = env.run_ok_in(&wt2, &["memory", "search", "legacy convention"]);
    assert!(out.contains("legacy convention"), "got: {out}");
    assert!(env.home.path().join(".graphmind/memory/main-wt.jsonl").exists());

    // The main checkout itself resolves exactly as before.
    let out = env.run_ok_in(&repo, &["status"]);
    assert_eq!(status_project(&out), "main-wt");
}

#[test]
fn same_named_worktrees_get_distinct_slugs() {
    let env = CliTestEnv::new();
    let repo = init_repo(&env, "a/app");
    env.run_ok_in(&repo, &["register", "."]);
    let other = env.home.path().join("b/app");
    git(&repo, &["worktree", "add", "-q", other.to_str().unwrap(), "-b", "b"]);

    env.run_ok_in(&other, &["status"]);

    let main = project_at(&env, &repo).expect("main checkout still registered");
    assert_eq!(main["slug"], "app");
    let wt = project_at(&env, &other).expect("worktree registered");
    assert_ne!(wt["slug"], "app", "worktree must not take over the main checkout's slug");
}

#[test]
fn reregistering_a_linked_worktree_keeps_its_slug_and_memory() {
    let env = CliTestEnv::new();
    let (repo, wt2) = init_repo_with_worktree(&env);
    env.run_ok_in(&repo, &["register", ".", "--slug", "main-wt"]);
    env.run_ok_in(&wt2, &["status"]);

    env.run_ok_in(&wt2, &["register", "."]);

    let cfg = config(&env);
    let at_wt2: Vec<_> = cfg["projects"]
        .as_object()
        .unwrap()
        .values()
        .filter(|p| p["path"].as_str() == Some(wt2.canonicalize().unwrap().to_str().unwrap()))
        .collect();
    assert_eq!(at_wt2.len(), 1, "one entry per path");
    assert_eq!(at_wt2[0]["slug"], "wt2");
    assert_eq!(at_wt2[0]["primary"], "main-wt");
}

#[test]
fn project_argument_accepts_a_checkout_path() {
    let env = CliTestEnv::new();
    let (repo, wt2) = init_repo_with_worktree(&env);
    env.run_ok_in(&repo, &["register", ".", "--slug", "main-wt"]);

    // How an MCP client in a worktree names its project.
    let out = env.run_ok(&["status", wt2.to_str().unwrap()]);
    assert_eq!(status_project(&out), "wt2", "got: {out}");

    // Given with --slug from inside the main checkout, the worktree wins over the current directory.
    let out = env.run_ok_in(&repo, &["status", "--slug", wt2.to_str().unwrap()]);
    assert_eq!(status_project(&out), "wt2", "got: {out}");
}

#[test]
fn worktrees_get_distinct_graphs() {
    let env = CliTestEnv::new();
    let (repo, wt2) = init_repo_with_worktree(&env);

    env.run_ok_in(&repo, &["register", ".", "--slug", "main-wt"]);
    env.run_ok_in(&wt2, &["register", ".", "--slug", "wt2-slug"]);

    env.run_ok_in(&repo, &["build", "main-wt"]);
    env.run_ok_in(&wt2, &["build", "wt2-slug"]);

    let graphs_dir = env.home.path().join(".graphmind").join("graphs");
    assert!(graphs_dir.join("main-wt").join("graph.db").exists());
    assert!(graphs_dir.join("wt2-slug").join("graph.db").exists());
}

#[test]
fn clean_stale_prunes_a_removed_worktree_but_keeps_live_ones() {
    let env = CliTestEnv::new();
    let (repo, wt2) = init_repo_with_worktree(&env);

    env.run_ok_in(&repo, &["register", ".", "--slug", "main-wt"]);
    env.run_ok_in(&wt2, &["register", ".", "--slug", "wt2-slug"]);

    let out = env.run_ok(&["clean", "--stale"]);
    assert!(!out.contains("Pruned"), "nothing is stale yet, got: {out}");

    git(&repo, &["worktree", "remove", wt2.to_str().unwrap(), "--force"]);

    let out = env.run_ok(&["clean", "--stale"]);
    assert!(out.contains("wt2-slug"), "got: {out}");
    let list = env.run_ok(&["list"]);
    assert!(list.contains("main-wt") && !list.contains("wt2-slug"), "got: {list}");
    assert!(!env.home.path().join(".graphmind/graphs/wt2-slug").exists());
}

#[test]
fn linking_forgets_auto_linked_worktrees_that_are_gone() {
    let env = CliTestEnv::new();
    let (repo, wt2) = init_repo_with_worktree(&env);
    env.run_ok_in(&repo, &["register", ".", "--slug", "main-wt"]);
    env.run_ok_in(&wt2, &["status"]);
    git(&repo, &["worktree", "remove", wt2.to_str().unwrap(), "--force"]);

    let wt3 = env.home.path().join("wt3");
    git(&repo, &["worktree", "add", "-q", wt3.to_str().unwrap(), "-b", "third"]);
    env.run_ok_in(&wt3, &["status"]);

    let list = env.run_ok(&["list"]);
    assert!(list.contains("wt3") && !list.contains("wt2"), "got: {list}");
    assert!(!env.home.path().join(".graphmind/graphs/wt2").exists());
}

/// A worktree removed with `git worktree remove` is forgotten by the next `graphmind list`,
/// without waiting for another worktree of the repo to be registered.
#[test]
fn list_forgets_a_removed_worktree() {
    let env = CliTestEnv::new();
    let (repo, wt2) = init_repo_with_worktree(&env);
    env.run_ok_in(&repo, &["register", ".", "--slug", "main-wt"]);
    env.run_ok_in(&wt2, &["status"]);
    assert!(env.run_ok(&["list"]).contains("wt2"));

    git(&repo, &["worktree", "remove", wt2.to_str().unwrap(), "--force"]);

    let list = env.run_ok(&["list"]);
    assert!(list.contains("main-wt") && !list.contains("wt2"), "got: {list}");
    assert!(!env.home.path().join(".graphmind/graphs/wt2").exists());
}

/// `build --all` forgets a removed worktree instead of failing on its missing folder.
#[test]
fn build_all_succeeds_after_a_worktree_is_removed() {
    let env = CliTestEnv::new();
    let (repo, wt2) = init_repo_with_worktree(&env);
    env.run_ok_in(&repo, &["register", ".", "--slug", "main-wt"]);
    env.run_ok_in(&wt2, &["status"]);
    git(&repo, &["worktree", "remove", wt2.to_str().unwrap(), "--force"]);

    let out = env.run(&["build", "--all"]);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(out.status.success(), "stderr: {stderr}");
    assert!(stderr.contains("removed worktree(s) forgotten: wt2"), "{stderr}");
    assert!(!env.run_ok(&["list"]).contains("wt2"));
}

/// A project registered by hand stays listed when its folder is gone: only worktrees graphmind
/// registered itself are forgotten on their own; `clean --stale` handles the rest.
#[test]
fn list_keeps_a_missing_project_registered_by_hand() {
    let env = CliTestEnv::new();
    let gone = env.home.path().join("gone-project");
    std::fs::create_dir_all(&gone).unwrap();
    std::fs::write(gone.join("lib.rs"), "pub fn f() {}\n").unwrap();
    env.run_ok(&["register", gone.to_str().unwrap(), "--slug", "by-hand"]);
    std::fs::remove_dir_all(&gone).unwrap();

    assert!(env.run_ok(&["list"]).contains("by-hand"));
    env.run_ok(&["clean", "--stale"]);
    assert!(!env.run_ok(&["list"]).contains("by-hand"));
}

#[test]
fn unrelated_git_repo_does_not_borrow_the_only_project() {
    let env = CliTestEnv::new();
    let fixture = env.fixture_path();
    env.run_ok(&["register", fixture.to_str().unwrap(), "--slug", "unrelated-project"]);

    let other = init_repo(&env, "some-other-repo");
    let out = env.run_in(&other, &["status"]);
    assert!(
        !out.status.success(),
        "another codebase must not silently answer from the only registered project"
    );
}

#[test]
fn only_project_still_resolves_outside_git_repos() {
    let env = CliTestEnv::new();
    let fixture = env.fixture_path();
    env.run_ok(&["register", fixture.to_str().unwrap(), "--slug", "only-project"]);

    // e.g. an MCP server Claude Desktop starts in `/`: unchanged behavior.
    let dir = env.home.path().join("not-a-repo");
    std::fs::create_dir_all(&dir).unwrap();
    let out = env.run_ok_in(&dir, &["status"]);
    assert_eq!(status_project(&out), "only-project");
}

#[test]
fn unreadable_config_is_never_replaced() {
    let env = CliTestEnv::new();
    let (repo, wt2) = init_repo_with_worktree(&env);
    env.run_ok_in(&repo, &["register", ".", "--slug", "main-wt"]);

    let path = env.home.path().join(".graphmind/config.json");
    let mut broken = std::fs::read_to_string(&path).unwrap();
    broken.truncate(broken.len() / 2);
    std::fs::write(&path, &broken).unwrap();

    let _ = env.run_in(&wt2, &["status"]);
    assert_eq!(std::fs::read_to_string(&path).unwrap(), broken);
}

#[test]
fn git_hook_environment_does_not_leak_into_other_projects() {
    let env = CliTestEnv::new();
    let (repo, wt2) = init_repo_with_worktree(&env);
    let other = init_repo(&env, "other");
    env.run_ok_in(&repo, &["register", ".", "--slug", "main-wt"]);
    env.run_ok_in(&other, &["register", ".", "--slug", "other"]);
    let other_id = project_at(&env, &other).unwrap()["repo_id"].clone();

    // An install from before worktree support: repo ids get backfilled.
    let path = env.home.path().join(".graphmind/config.json");
    let mut cfg = config(&env);
    for p in cfg["projects"].as_object_mut().unwrap().values_mut() {
        p.as_object_mut().unwrap().remove("repo_id");
    }
    std::fs::write(&path, serde_json::to_string_pretty(&cfg).unwrap()).unwrap();

    // Git exports GIT_DIR to the hooks it runs; here, the new worktree's.
    let git_dir = String::from_utf8(
        Command::new("git").arg("-C").arg(&wt2).args(["rev-parse", "--absolute-git-dir"]).output().unwrap().stdout,
    )
    .unwrap();
    let out = Command::new(&env.binary)
        .arg("status")
        .current_dir(&wt2)
        .env("HOME", env.home.path())
        .env_remove("CLAUDE_CONFIG_DIR")
        .env("GRAPHMIND_SKIP_NOTICES", "1")
        .env("GRAPHMIND_NO_AUTO_BUILD", "1")
        .env("GIT_DIR", git_dir.trim())
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert_eq!(status_project(&stdout), "wt2", "got: {stdout}");
    assert_eq!(
        project_at(&env, &other).unwrap()["repo_id"], other_id,
        "another project's identity must come from its own repo"
    );
}

#[test]
fn clean_stale_keeps_projects_registered_below_a_checkout_root() {
    let env = CliTestEnv::new();
    let repo = init_repo(&env, "mono");
    let api = repo.join("packages/api");
    std::fs::create_dir_all(&api).unwrap();
    env.run_ok(&["register", api.to_str().unwrap(), "--slug", "api"]);

    let out = env.run_ok(&["clean", "--stale"]);
    assert!(!out.contains("Pruned"), "a monorepo package is not a removed worktree: {out}");
    assert!(env.run_ok(&["list"]).contains("api"));
}

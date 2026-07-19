mod common;
use common::CliTestEnv;
use std::process::Command;

fn git(dir: &std::path::Path, args: &[&str]) {
    let status = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .env("GIT_AUTHOR_NAME", "test")
        .env("GIT_AUTHOR_EMAIL", "test@test.com")
        .env("GIT_COMMITTER_NAME", "test")
        .env("GIT_COMMITTER_EMAIL", "test@test.com")
        .status()
        .expect("failed to run git");
    assert!(status.success(), "git {args:?} failed in {dir:?}");
}

/// Sets up a real git repo (main worktree at `repo/`, with one commit) and
/// a second worktree at `wt2/` on branch `feature`. Returns (repo, wt2).
fn init_repo_with_worktree(env: &CliTestEnv) -> (std::path::PathBuf, std::path::PathBuf) {
    let base = env.home.path().to_path_buf();
    let repo = base.join("repo");
    std::fs::create_dir_all(&repo).unwrap();
    git(&repo, &["init", "-q"]);
    git(&repo, &["config", "commit.gpgsign", "false"]);
    std::fs::write(repo.join("f.txt"), "hello").unwrap();
    git(&repo, &["add", "."]);
    git(&repo, &["commit", "-q", "-m", "init"]);

    let wt2 = base.join("wt2");
    git(&repo, &["worktree", "add", "-q", wt2.to_str().unwrap(), "-b", "feature"]);

    (repo, wt2)
}

#[test]
fn unregistered_worktree_auto_links_to_sibling_repo() {
    let env = CliTestEnv::new();
    let (repo, wt2) = init_repo_with_worktree(&env);

    env.run_ok_in(&repo, &["register", ".", "--slug", "main-wt"]);

    // wt2 was never explicitly registered — a slug-less command run from
    // inside it should auto-link to main-wt's repo rather than erroring
    // (proven by identical git-common-dir) or silently borrowing an
    // unrelated project.
    let out = env.run_ok_in(&wt2, &["status"]);
    assert!(
        out.contains("wt2") || out.contains("repo_id") || out.contains("Project"),
        "status from unregistered worktree should auto-link, got: {out}"
    );

    let list = env.run_ok(&["list"]);
    assert!(list.contains("main-wt"), "main-wt should still be registered, got: {list}");
    assert!(
        list.matches("registered project").count() >= 1,
        "list output should show at least one registered project, got: {list}"
    );
}

#[test]
fn memory_is_shared_across_worktrees() {
    let env = CliTestEnv::new();
    let (repo, wt2) = init_repo_with_worktree(&env);

    env.run_ok_in(&repo, &["register", ".", "--slug", "main-wt"]);
    env.run_ok_in(&repo, &["memory", "add", "decision made in main worktree", "--type", "decision", "--in", "main-wt"]);

    // Auto-link wt2 by running a slug-less command from inside it.
    env.run_ok_in(&wt2, &["memory", "add", "note added from feature worktree", "--type", "context"]);

    // A memory search from the main worktree should find the entry added
    // from wt2 — proving they share one memory store keyed by repo_id.
    let out = env.run_ok_in(&repo, &["memory", "search", "feature worktree", "--in", "main-wt"]);
    assert!(
        out.contains("note added from feature worktree"),
        "memory added from wt2 should be visible from main-wt, got: {out}"
    );

    // Exactly one memory file should exist for this repo (repo_id-keyed),
    // not two separate per-slug files.
    let mem_dir = env.home.path().join(".graphmind").join("memory");
    let jsonl_files: Vec<_> = std::fs::read_dir(&mem_dir)
        .unwrap()
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().and_then(|x| x.to_str()) == Some("jsonl"))
        .filter(|e| e.file_name() != "global.jsonl")
        .collect();
    assert_eq!(
        jsonl_files.len(),
        1,
        "expected exactly one shared project memory file, found: {:?}",
        jsonl_files.iter().map(|e| e.file_name()).collect::<Vec<_>>()
    );
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
    assert!(graphs_dir.join("main-wt").join("graph.db").exists(), "main-wt should have its own graph.db");
    assert!(graphs_dir.join("wt2-slug").join("graph.db").exists(), "wt2-slug should have its own graph.db");
}

#[test]
fn truly_unrelated_cwd_does_not_silently_borrow_a_project() {
    let env = CliTestEnv::new();
    let fixture = env.fixture_path();
    env.run_ok(&["register", fixture.to_str().unwrap(), "--slug", "unrelated-project"]);

    // A directory with no git repo and no relation to any registered
    // project must NOT silently resolve to "the only registered project"
    // (the removed len()==1 fallback) — it should fail with an explicit
    // error instead.
    let empty_dir = env.home.path().join("nothing-here");
    std::fs::create_dir_all(&empty_dir).unwrap();

    let out = env.run_in(&empty_dir, &["status"]);
    assert!(
        !out.status.success(),
        "status from an unrelated, unregistered, non-git directory should fail, not silently succeed"
    );
}

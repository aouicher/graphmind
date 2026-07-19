use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::config::slugify;

fn run_git(path: &Path, args: &[&str]) -> Option<String> {
    let output = Command::new("git")
        .arg("-C")
        .arg(path)
        .args(args)
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if stdout.is_empty() {
        None
    } else {
        Some(stdout)
    }
}

/// The shared git directory for `path`'s repo. For a worktree this resolves
/// to the main repo's `.git`, not the worktree-local pointer file — git
/// itself does this resolution, so we shell out rather than hand-parsing
/// the `gitdir:` pointer.
pub fn common_dir(path: &Path) -> Option<PathBuf> {
    let raw = run_git(path, &["rev-parse", "--git-common-dir"])?;
    let candidate = PathBuf::from(raw);
    let absolute = if candidate.is_absolute() {
        candidate
    } else {
        path.join(candidate)
    };
    absolute.canonicalize().ok()
}

/// A stable identifier for the logical repo at `path`, shared by every
/// worktree of that repo. `None` if `path` isn't inside a git repo (or git
/// isn't available) — callers should fall back to path-only behavior.
pub fn repo_id(path: &Path) -> Option<String> {
    let common = common_dir(path)?;
    let mut hasher = Sha256::new();
    hasher.update(common.to_string_lossy().as_bytes());
    let hash = format!("{:x}", hasher.finalize());
    let short_hash = &hash[..12];

    let basename = common
        .parent()
        .and_then(|p| p.file_name())
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_default();
    let prefix = slugify(&basename);

    if prefix.is_empty() {
        Some(short_hash.to_string())
    } else {
        Some(format!("{prefix}-{short_hash}"))
    }
}

/// The hooks directory for `path`'s repo, correctly resolved for worktrees
/// (shared with the main checkout) and `core.hooksPath` overrides.
pub fn hooks_dir(path: &Path) -> Option<PathBuf> {
    let raw = run_git(path, &["rev-parse", "--git-path", "hooks"])?;
    let candidate = PathBuf::from(raw);
    let absolute = if candidate.is_absolute() {
        candidate
    } else {
        path.join(candidate)
    };
    // Canonicalize so worktree and main-checkout callers agree byte-for-byte
    // (e.g. macOS resolves /var -> /private/var only when a symlink is
    // actually traversed, which differs between the two invocation paths).
    // The dir may not exist yet (hooks/ is created lazily), so fall back to
    // the uncanonicalized absolute path if canonicalize fails.
    Some(absolute.canonicalize().unwrap_or(absolute))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command;

    fn git(dir: &Path, args: &[&str]) {
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

    #[test]
    fn repo_id_matches_across_worktrees() {
        let base = tempfile::tempdir().unwrap();
        let repo = base.path().join("repo");
        std::fs::create_dir_all(&repo).unwrap();
        git(&repo, &["init", "-q"]);
        git(&repo, &["config", "commit.gpgsign", "false"]);
        std::fs::write(repo.join("f.txt"), "hello").unwrap();
        git(&repo, &["add", "."]);
        git(&repo, &["commit", "-q", "-m", "init"]);

        let worktree = base.path().join("worktree");
        git(&repo, &["worktree", "add", "-q", worktree.to_str().unwrap(), "-b", "wt-branch"]);

        let main_id = repo_id(&repo).expect("main repo_id");
        let wt_id = repo_id(&worktree).expect("worktree repo_id");
        assert_eq!(main_id, wt_id, "repo_id must match across worktrees");
    }

    #[test]
    fn repo_id_none_outside_git_repo() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(repo_id(dir.path()), None);
    }

    #[test]
    fn hooks_dir_shared_across_worktrees() {
        let base = tempfile::tempdir().unwrap();
        let repo = base.path().join("repo");
        std::fs::create_dir_all(&repo).unwrap();
        git(&repo, &["init", "-q"]);
        git(&repo, &["config", "commit.gpgsign", "false"]);
        std::fs::write(repo.join("f.txt"), "hello").unwrap();
        git(&repo, &["add", "."]);
        git(&repo, &["commit", "-q", "-m", "init"]);

        let worktree = base.path().join("worktree");
        git(&repo, &["worktree", "add", "-q", worktree.to_str().unwrap(), "-b", "wt-branch"]);

        let main_hooks = hooks_dir(&repo).expect("main hooks_dir");
        let wt_hooks = hooks_dir(&worktree).expect("worktree hooks_dir");
        assert_eq!(main_hooks, wt_hooks, "hooks dir must be shared across worktrees");
        assert!(main_hooks.ends_with("hooks"));
    }
}

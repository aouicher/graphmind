use crate::config::Registry;
use crate::git_identity;

pub fn resolve_project_slug(candidates: &[Option<&str>]) -> Option<String> {
    for candidate in candidates.iter().flatten() {
        if !candidate.is_empty() && Registry::get(candidate).is_some() {
            return Some(candidate.to_string());
        }
    }

    let cwd = std::env::current_dir().ok();

    if let Some(cwd) = &cwd {
        if let Some(project) = Registry::find_by_path(&cwd.to_string_lossy()) {
            return Some(project.slug);
        }
    }

    // cwd isn't registered directly, but it may be an unregistered
    // worktree of an already-known repo — link it to that repo's identity
    // rather than erroring. This is safe (unlike the old len()==1
    // fallback) because it's provably the same repo, not a guess.
    if let Some(cwd) = &cwd {
        if let Some(repo_id) = git_identity::repo_id(cwd) {
            if let Some(sibling) = Registry::find_by_repo_id(&repo_id) {
                let project = Registry::register_worktree(&cwd.to_string_lossy(), &repo_id, &sibling);
                eprintln!(
                    "note: new worktree of '{}' auto-registered as '{}' — memory is shared, run `graphmind build` to index its graph",
                    sibling.slug, project.slug
                );
                return Some(project.slug);
            }
        }
    }

    // Truly unknown cwd — do NOT fall back to "the only registered
    // project" — that silently serves an unrelated project's graph when
    // cwd is e.g. an unregistered git worktree. Callers must get None and
    // surface an explicit "not registered" error instead.
    None
}

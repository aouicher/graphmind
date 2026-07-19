use crate::config::Registry;

pub fn resolve_project_slug(candidates: &[Option<&str>]) -> Option<String> {
    for candidate in candidates.iter().flatten() {
        if !candidate.is_empty() && Registry::get(candidate).is_some() {
            return Some(candidate.to_string());
        }
    }

    if let Ok(cwd) = std::env::current_dir() {
        if let Some(project) = Registry::find_by_path(&cwd.to_string_lossy()) {
            return Some(project.slug);
        }
    }

    // No exact path match. Do NOT fall back to "the only registered
    // project" — that silently serves an unrelated project's graph when
    // cwd is e.g. an unregistered git worktree. Callers must get None and
    // surface an explicit "not registered" error instead.
    None
}

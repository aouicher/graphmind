use std::path::Path;

use crate::config::Registry;
use crate::git_identity;

/// The project a command or tool call targets: the first candidate that is a
/// registered slug or a directory path, else the current directory. A directory
/// resolves to the registered project containing it, or — for another worktree
/// of a registered repo — to that worktree, registered on the spot.
pub fn resolve_project_slug(candidates: &[Option<&str>]) -> Option<String> {
    for candidate in candidates.iter().flatten() {
        if candidate.is_empty() {
            continue;
        }
        if Registry::get(candidate).is_some() {
            return Some(candidate.to_string());
        }
        // MCP clients may pass a path: an agent in a worktree knows its
        // directory, not graphmind's name for it.
        if let Some(slug) = resolve_path(candidate) {
            return Some(slug);
        }
    }

    let cwd = std::env::current_dir().ok()?;
    if let Some(slug) = resolve_dir(&cwd) {
        return Some(slug);
    }

    // With a single registered project, commands run from outside any git repo
    // (an MCP server Claude Desktop starts in `/`, a shell in $HOME) use it, as
    // they always have. Never from inside a git repo: that is another codebase,
    // and answering from this project's graph would be silently wrong.
    let projects = Registry::list();
    if projects.len() == 1 && git_identity::work_tree(&cwd).is_none() {
        return Some(projects[0].slug.clone());
    }

    None
}

/// The project for `candidate` when it is a directory path (not a slug), as
/// [`resolve_project_slug`] resolves the current directory.
pub fn resolve_path(candidate: &str) -> Option<String> {
    if candidate.contains(['/', '\\']) && Path::new(candidate).is_dir() {
        resolve_dir(Path::new(candidate))
    } else {
        None
    }
}

fn resolve_dir(dir: &Path) -> Option<String> {
    if let Some(project) = Registry::find_by_path(&dir.to_string_lossy()) {
        return Some(project.slug);
    }
    let project = Registry::link_worktree(dir)?;
    eprintln!(
        "note: worktree {} registered as '{}' (memory shared with '{}')",
        project.path,
        project.slug,
        project.primary.as_deref().unwrap_or("-"),
    );
    Some(project.slug)
}

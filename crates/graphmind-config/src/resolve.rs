use std::fmt;
use std::path::Path;

use crate::config::Registry;
use crate::git_identity;

/// A project given explicitly (a slug or a directory path) that names no registered project.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnknownProject(pub String);

impl fmt::Display for UnknownProject {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Project '{}' is not registered (see `graphmind list`)", self.0)
    }
}

impl std::error::Error for UnknownProject {}

/// The project a command or tool call targets: the first candidate that is a
/// registered slug or a directory path. A directory resolves to the registered
/// project containing it, or — for another worktree of a registered repo — to
/// that worktree, registered on the spot. Candidates given but none resolving
/// is an error, never the current directory's project. Without candidates, the
/// current directory decides, then the only registered project when the current
/// directory is in no git repo.
pub fn try_resolve_project_slug(candidates: &[Option<&str>]) -> Result<Option<String>, UnknownProject> {
    let mut unknown = None;
    for candidate in candidates.iter().flatten() {
        if candidate.is_empty() {
            continue;
        }
        if Registry::get(candidate).is_some() {
            return Ok(Some(candidate.to_string()));
        }
        if let Some(slug) = resolve_path(candidate) {
            return Ok(Some(slug));
        }
        unknown.get_or_insert(*candidate);
    }
    if let Some(candidate) = unknown {
        return Err(UnknownProject(candidate.to_string()));
    }

    let Ok(cwd) = std::env::current_dir() else {
        return Ok(None);
    };
    if let Some(slug) = resolve_dir(&cwd) {
        return Ok(Some(slug));
    }

    let projects = Registry::list();
    if projects.len() == 1 && git_identity::work_tree(&cwd).is_none() {
        return Ok(Some(projects[0].slug.clone()));
    }

    Ok(None)
}

/// The project for `candidate` when it is a directory path (not a slug), as
/// [`try_resolve_project_slug`] resolves the current directory. A path contains a
/// separator or is `.` or `..`; relative paths are taken from the current directory.
pub fn resolve_path(candidate: &str) -> Option<String> {
    let is_path = candidate.contains(['/', '\\']) || candidate == "." || candidate == "..";
    if is_path && Path::new(candidate).is_dir() {
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

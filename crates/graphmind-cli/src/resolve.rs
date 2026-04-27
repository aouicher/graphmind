use crate::config::Registry;

pub fn resolve_project_slug(candidates: &[Option<&str>]) -> Option<String> {
    // Try each candidate in order
    for candidate in candidates.iter().flatten() {
        if !candidate.is_empty() && Registry::get(candidate).is_some() {
            return Some(candidate.to_string());
        }
    }

    // Try finding by current working directory
    if let Ok(cwd) = std::env::current_dir() {
        if let Some(project) = Registry::find_by_path(&cwd.to_string_lossy()) {
            return Some(project.slug);
        }
    }

    // Fallback: if only one project is registered, use it
    let projects = Registry::list();
    if projects.len() == 1 {
        return Some(projects[0].slug.clone());
    }

    None
}

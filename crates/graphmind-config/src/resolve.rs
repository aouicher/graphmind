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

    let projects = Registry::list();
    if projects.len() == 1 {
        return Some(projects[0].slug.clone());
    }

    None
}

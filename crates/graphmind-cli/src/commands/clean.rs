use graphmind_config::{git_identity, paths, Registry, resolve_project_slug};
use colored::Colorize;
use std::path::Path;

pub fn clean(slug: Option<&str>, all: bool) {
    if all {
        let projects = Registry::list();
        if projects.is_empty() {
            println!("{}", "No projects registered.".dimmed());
            return;
        }
        for p in &projects {
            clean_single(&p.slug);
        }
        return;
    }

    let slug = match resolve_project_slug(&[slug]) {
        Some(s) => s,
        None => {
            eprintln!(
                "{} No project specified and none could be resolved",
                "Error:".red().bold()
            );
            std::process::exit(1);
        }
    };

    clean_single(&slug);
}

/// `graphmind clean --stale` — unregisters projects whose worktree is
/// gone: either the registered path no longer exists on disk, or (for a
/// project with a repo_id) `git worktree list` for a still-existing
/// sibling no longer lists this path — i.e. `git worktree remove` was run
/// but the directory happened to survive some other way. Explicit opt-in
/// only; never runs implicitly, since unregistering is destructive.
pub fn clean_stale() {
    let projects = Registry::list();
    if projects.is_empty() {
        println!("{}", "No projects registered.".dimmed());
        return;
    }

    let mut pruned = Vec::new();

    for p in &projects {
        let path_exists = Path::new(&p.path).exists();

        let is_stale = if !path_exists {
            true
        } else if let Some(repo_id) = &p.repo_id {
            // Ask git (via this project's own path, or any living sibling
            // sharing the same repo_id) whether it still lists this path
            // as a worktree. If git can't answer at all, don't prune —
            // "unknown" must never be treated as "stale".
            let known_worktrees = git_identity::list_worktrees(Path::new(&p.path)).or_else(|| {
                projects
                    .iter()
                    .filter(|sibling| sibling.slug != p.slug && sibling.repo_id.as_deref() == Some(repo_id.as_str()))
                    .find_map(|sibling| git_identity::list_worktrees(Path::new(&sibling.path)))
            });
            match known_worktrees {
                Some(worktrees) => {
                    let this_path = Path::new(&p.path).canonicalize().unwrap_or_else(|_| Path::new(&p.path).to_path_buf());
                    !worktrees.contains(&this_path)
                }
                None => false,
            }
        } else {
            false
        };

        if is_stale && Registry::unregister(&p.slug) {
            let graph_dir = paths::graph_dir(&p.slug);
            if graph_dir.exists() {
                std::fs::remove_dir_all(&graph_dir).ok();
            }
            pruned.push(p.slug.clone());
        }
    }

    if pruned.is_empty() {
        println!("{}", "No stale projects found.".dimmed());
    } else {
        println!(
            "{} Pruned {} stale project(s): {}",
            "OK".green().bold(),
            pruned.len(),
            pruned.join(", ").cyan()
        );
    }
}

fn clean_single(slug: &str) {
    let graph_dir = paths::graph_dir(slug);
    if graph_dir.exists() {
        std::fs::remove_dir_all(&graph_dir).ok();
        std::fs::create_dir_all(&graph_dir).ok();
        Registry::update_project(slug, |p| {
            p.last_build = None;
        });
        println!("{} Cleaned graph for {}", "OK".green().bold(), slug.cyan());
    } else {
        println!("{} No graph found for {}", "!".yellow(), slug);
    }
}

use crate::config::Registry;
use crate::paths;
use crate::resolve::resolve_project_slug;
use colored::Colorize;

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

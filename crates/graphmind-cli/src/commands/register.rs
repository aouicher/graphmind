use chrono::{DateTime, Local};
use colored::Colorize;
use graphmind_config::{paths, ProjectConfig, Registry};
use std::path::Path;
use super::resolve_project_slug;

fn fmt_local(utc: &str) -> String {
    DateTime::parse_from_rfc3339(utc)
        .map(|dt| dt.with_timezone(&Local).format("%Y-%m-%d %H:%M:%S").to_string())
        .unwrap_or_else(|_| utc.to_string())
}

/// `graphmind register`: the project, and its Claude Code MCP entry, as `graphmind init` writes it.
/// A .claude.json that can't be updated is reported and the registration stands.
pub fn register(path: &str, slug: Option<&str>, exclude: &[String]) {
    let project = add(path, slug, exclude);
    super::setup::install_claude_code_project(Path::new(&project.path));
}

/// Register the project and say so. `graphmind init` writes the MCP entries in a step of its own.
pub fn add(path: &str, slug: Option<&str>, exclude: &[String]) -> ProjectConfig {
    let project = Registry::register(path, slug, exclude);
    println!(
        "{} Registered project {} at {}",
        "OK".green().bold(),
        project.slug.cyan(),
        project.path.dimmed()
    );
    project
}

pub fn unregister(slug: Option<&str>) {
    let slug = match resolve_project_slug(&[slug]) {
        Some(s) => s,
        None => {
            eprintln!("{} No project specified and none could be resolved", "Error:".red().bold());
            std::process::exit(1);
        }
    };

    let project_path = Registry::get(&slug).map(|p| p.path);
    if Registry::unregister(&slug) {
        // Remove graph directory
        let graph_dir = paths::graph_dir(&slug);
        if graph_dir.exists() {
            std::fs::remove_dir_all(&graph_dir).ok();
        }
        println!("{} Unregistered project {}", "OK".green().bold(), slug.cyan());
        if let Some(project_path) = project_path {
            super::setup::uninstall_claude_code_project(Path::new(&project_path));
        }
    } else {
        eprintln!("{} Project {} not found", "Error:".red().bold(), slug);
        std::process::exit(1);
    }
}

pub fn list() {
    forget_gone_worktrees();
    let projects = Registry::list();
    if projects.is_empty() {
        println!("{}", "No projects registered. Use 'graphmind register' to add one.".dimmed());
        return;
    }

    println!("{} registered project(s):\n", projects.len().to_string().cyan().bold());
    for p in &projects {
        let build_info_owned = p.last_build.as_deref().map(fmt_local).unwrap_or_else(|| "never".to_string());
        let build_info = build_info_owned.as_str();
        let worktree = match (&p.branch, &p.primary) {
            (Some(b), Some(primary)) => format!("  [{b}, worktree of {primary}]"),
            (Some(b), None) => format!("  [{b}]"),
            (None, Some(primary)) => format!("  [worktree of {primary}]"),
            (None, None) => String::new(),
        };
        let opt_out = if p.link_worktrees { "" } else { "  [worktrees not linked]" };
        println!(
            "  {} {}{}{}",
            p.slug.cyan().bold(),
            p.path.dimmed(),
            worktree.dimmed(),
            opt_out.dimmed()
        );
        println!(
            "    last build: {}  languages: {}",
            build_info.yellow(),
            if p.languages.is_empty() {
                "-".to_string()
            } else {
                p.languages.join(", ")
            }
        );
    }
}

/// `graphmind which` — slug of the registered project that contains the current directory.
/// No fallback to "the only registered project": hooks use this to decide whether to run.
pub fn which() {
    // resolve_path, not resolve_project_slug: outside any git repo the latter falls back to the
    // single registered project, which would make hooks run in unrelated directories. Unlike
    // Registry::find_by_path, it also registers a new worktree of a registered repo.
    let cwd = std::env::current_dir().ok();
    match cwd.and_then(|d| graphmind_config::resolve_path(&d.to_string_lossy())) {
        Some(slug) => println!("{slug}"),
        None => std::process::exit(1),
    }
}

pub fn status(slug: Option<&str>) {
    let slug = match resolve_project_slug(&[slug]) {
        Some(s) => s,
        None => {
            eprintln!("{} No project specified and none could be resolved", "Error:".red().bold());
            std::process::exit(1);
        }
    };

    let project = match Registry::get(&slug) {
        Some(p) => p,
        None => {
            eprintln!("{} Project {} not found", "Error:".red().bold(), slug);
            std::process::exit(1);
        }
    };

    println!("{} {}", "Project:".bold(), project.slug.cyan().bold());
    println!("  {}: {}", "Path".bold(), project.path);
    println!("  {}: {}", "Registered".bold(), project.registered);
    println!(
        "  {}: {}",
        "Last build".bold(),
        project.last_build.as_deref().map(fmt_local).unwrap_or_else(|| "never".to_string()).yellow()
    );

    let db_path = paths::graph_db_path(&slug);
    if db_path.exists() {
        let db_path_str = db_path.to_string_lossy().to_string();
        if graphmind_db::schema::schema_needs_reset(&db_path_str) {
            println!(
                "  {}: {}",
                "Schema".bold(),
                "outdated — run `graphmind build --reset` to reindex".yellow()
            );
        }
        if let Ok(db) = graphmind_db::schema::init_database(&db_path_str) {
            let queries = graphmind_db::queries::GraphQueries::new(&db);
            let stats = queries.stats();
            println!(
                "  {}: {} symbols, {} edges, {} files",
                "Graph".bold(),
                stats.symbols.to_string().green(),
                stats.edges.to_string().green(),
                stats.files.to_string().green()
            );
            let langs = queries.language_breakdown();
            if !langs.is_empty() {
                let lang_str: Vec<String> = langs
                    .iter()
                    .map(|l| format!("{} ({})", l.language, l.count))
                    .collect();
                println!("  {}: {}", "Languages".bold(), lang_str.join(", "));
            }
        }
    } else {
        println!("  {}: {}", "Graph".bold(), "not built yet".yellow());
    }
}

/// Forgets the worktrees removed since they were registered, saying so, so that listing and
/// building every project don't trip over them.
pub fn forget_gone_worktrees() {
    let gone = Registry::forget_gone_worktrees();
    if !gone.is_empty() {
        eprintln!("{} removed worktree(s) forgotten: {}", "→".cyan(), gone.join(", "));
    }
}

/// `graphmind link-worktrees [on|off]`: whether new git worktrees of the project's repo are
/// registered automatically when a command runs in one. Repo-wide: set on a worktree, it applies
/// to the repo. Without a state, prints the setting.
pub fn link_worktrees(state: Option<&str>, in_slug: Option<&str>) {
    // Never resolve the usual way: run in a new worktree, that would register (and build) it first.
    let project = match in_slug {
        Some(s) => Registry::get(s).or_else(|| {
            let dir = Path::new(s);
            if dir.is_dir() { Registry::find_without_linking(dir) } else { None }
        }),
        None => std::env::current_dir().ok().and_then(|d| Registry::find_without_linking(&d)),
    };
    let Some(project) = project else {
        eprintln!(
            "{} No registered project here{}. Pass --slug <project>",
            "Error:".red().bold(),
            in_slug.map(|s| format!(" named '{s}'")).unwrap_or_default()
        );
        std::process::exit(1);
    };
    let label = |on: bool| if on { "on" } else { "off" };
    let main = project.primary.clone().unwrap_or_else(|| project.slug.clone());
    let Some(state) = state else {
        println!("{}: link-worktrees {}", main.cyan(), label(Registry::worktrees_linked(&project.slug)));
        return;
    };
    let on = state == "on";
    if Registry::set_link_worktrees(&project.slug, on).is_none() {
        eprintln!("{} Could not update the config for '{}'", "Error:".red().bold(), project.slug);
        std::process::exit(1);
    }
    println!("{} link-worktrees {} for {} and its worktrees", "OK".green().bold(), label(on), main.cyan());
    let linked = Registry::linked_worktrees(&main);
    if !on && !linked.is_empty() {
        println!(
            "  {} worktrees registered before stay registered and keep building; to drop one: graphmind unregister <slug>",
            "note:".dimmed()
        );
        for w in &linked {
            println!("    {} {}", w.slug.cyan(), w.path.dimmed());
        }
    }
}

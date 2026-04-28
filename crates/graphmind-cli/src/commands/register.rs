use graphmind_config::{paths, Registry, resolve_project_slug};
use colored::Colorize;

pub fn register(path: &str, slug: Option<&str>, exclude: &[String]) {
    let project = Registry::register(path, slug, exclude);
    println!(
        "{} Registered project {} at {}",
        "OK".green().bold(),
        project.slug.cyan(),
        project.path.dimmed()
    );
}

pub fn unregister(slug: Option<&str>) {
    let slug = match resolve_project_slug(&[slug]) {
        Some(s) => s,
        None => {
            eprintln!("{} No project specified and none could be resolved", "Error:".red().bold());
            std::process::exit(1);
        }
    };

    if Registry::unregister(&slug) {
        // Remove graph directory
        let graph_dir = paths::graph_dir(&slug);
        if graph_dir.exists() {
            std::fs::remove_dir_all(&graph_dir).ok();
        }
        println!("{} Unregistered project {}", "OK".green().bold(), slug.cyan());
    } else {
        eprintln!("{} Project {} not found", "Error:".red().bold(), slug);
        std::process::exit(1);
    }
}

pub fn list() {
    let projects = Registry::list();
    if projects.is_empty() {
        println!("{}", "No projects registered. Use 'graphmind register' to add one.".dimmed());
        return;
    }

    println!("{} registered project(s):\n", projects.len().to_string().cyan().bold());
    for p in &projects {
        let build_info = p
            .last_build
            .as_deref()
            .unwrap_or("never");
        println!(
            "  {} {}",
            p.slug.cyan().bold(),
            p.path.dimmed()
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
        project.last_build.as_deref().unwrap_or("never").yellow()
    );

    let db_path = paths::graph_db_path(&slug);
    if db_path.exists() {
        let db_path_str = db_path.to_string_lossy().to_string();
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

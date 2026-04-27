use crate::paths;
use crate::resolve::resolve_project_slug;
use colored::Colorize;
use graphmind_db::queries::GraphQueries;
use graphmind_db::schema::init_database;

pub fn search(query: &str, slug: Option<&str>, limit: i64) {
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

    let db_path = paths::graph_db_path(&slug);
    if !db_path.exists() {
        eprintln!(
            "{} No graph found for {}. Run 'graphmind build' first.",
            "Error:".red().bold(),
            slug
        );
        std::process::exit(1);
    }

    let db_path_str = db_path.to_string_lossy().to_string();
    let db = init_database(&db_path_str).unwrap_or_else(|e| {
        eprintln!("{} Failed to open database: {}", "Error:".red().bold(), e);
        std::process::exit(1);
    });

    let q = GraphQueries::new(&db);

    // Convert natural language query to FTS-compatible query
    let fts_query = query
        .split(';')
        .flat_map(|part| part.split_whitespace())
        .filter(|w| w.len() > 1)
        .collect::<Vec<_>>()
        .join(" OR ");

    if fts_query.is_empty() {
        println!("{} Empty search query", "!".yellow());
        return;
    }

    let results = q.search_symbols(&fts_query, limit);

    if results.is_empty() {
        println!("{} No results for: {}", "!".yellow(), query);
        return;
    }

    println!(
        "{} {} result(s) for \"{}\":\n",
        ">>".cyan().bold(),
        results.len().to_string().green(),
        query
    );

    for s in &results {
        println!(
            "  {} [{}] {}:{}",
            s.name.bold(),
            s.kind.yellow(),
            s.file.dimmed(),
            s.line_start
        );
        if let Some(ref sig) = s.signature {
            if !sig.is_empty() {
                println!("    {}", sig.dimmed());
            }
        }
    }
}

pub fn embed() {
    println!("{}", "Embedding search coming soon".yellow());
}

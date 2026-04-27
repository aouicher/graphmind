use crate::paths;
use crate::resolve::resolve_project_slug;
use colored::Colorize;
use graphmind_db::queries::GraphQueries;
use graphmind_db::schema::init_database;
use std::process::Command;

pub fn diff_impact(slug: Option<&str>, staged: bool, depth: usize) {
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

    // Get changed files from git
    let changed_files = get_changed_files(staged);
    if changed_files.is_empty() {
        println!("{} No changed files detected", "!".yellow());
        return;
    }

    println!(
        "{} {} changed file(s):",
        ">>".cyan().bold(),
        changed_files.len().to_string().green()
    );
    for f in &changed_files {
        println!("  {}", f.yellow());
    }

    // Open the database and compute impact
    let db_path_str = db_path.to_string_lossy().to_string();
    let db = init_database(&db_path_str).unwrap_or_else(|e| {
        eprintln!("{} Failed to open database: {}", "Error:".red().bold(), e);
        std::process::exit(1);
    });

    let q = GraphQueries::new(&db);
    let mut all_impacted = std::collections::HashSet::new();

    for file in &changed_files {
        let impacted = q.impact(file, depth);
        for f in impacted {
            all_impacted.insert(f);
        }
    }

    // Remove changed files themselves from impact list
    for f in &changed_files {
        all_impacted.remove(f);
    }

    if all_impacted.is_empty() {
        println!("\n{} No additional files impacted", "OK".green().bold());
    } else {
        println!(
            "\n{} {} additional file(s) impacted (depth {}):",
            "!".yellow().bold(),
            all_impacted.len().to_string().red(),
            depth
        );
        for f in &all_impacted {
            println!("  {f}");
        }
    }
}

fn get_changed_files(staged: bool) -> Vec<String> {
    let args = if staged {
        vec!["diff", "--cached", "--name-only"]
    } else {
        vec!["diff", "--name-only", "HEAD"]
    };

    let output = Command::new("git")
        .args(&args)
        .output();

    match output {
        Ok(out) => {
            let stdout = String::from_utf8_lossy(&out.stdout);
            stdout
                .lines()
                .map(|l| l.trim().to_string())
                .filter(|l| !l.is_empty())
                .collect()
        }
        Err(_) => {
            // Fallback: try unstaged changes
            let fallback = Command::new("git")
                .args(["diff", "--name-only"])
                .output();
            match fallback {
                Ok(out) => {
                    let stdout = String::from_utf8_lossy(&out.stdout);
                    stdout
                        .lines()
                        .map(|l| l.trim().to_string())
                        .filter(|l| !l.is_empty())
                        .collect()
                }
                Err(_) => Vec::new(),
            }
        }
    }
}

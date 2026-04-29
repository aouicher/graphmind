use graphmind_config::{paths, resolve_project_slug};
use colored::Colorize;
use graphmind_db::queries::GraphQueries;
use graphmind_db::schema::init_database;

fn open_db(slug: &str) -> rusqlite::Connection {
    let db_path = paths::graph_db_path(slug);
    let db_path_str = db_path.to_string_lossy().to_string();
    if !db_path.exists() {
        eprintln!(
            "{} No graph found for {}. Run 'graphmind build' first.",
            "Error:".red().bold(),
            slug
        );
        std::process::exit(1);
    }
    init_database(&db_path_str).unwrap_or_else(|e| {
        eprintln!("{} Failed to open database: {}", "Error:".red().bold(), e);
        std::process::exit(1);
    })
}

pub fn query_symbol(name: &str, slug: Option<&str>) {
    let slug = require_slug(slug);
    let db = open_db(&slug);
    let q = GraphQueries::new(&db);

    let symbols = q.find_symbol(name);
    if symbols.is_empty() {
        println!("{} No symbol found: {}", "!".yellow(), name);
        return;
    }

    for s in &symbols {
        println!(
            "{} {} [{}] in {}:{}",
            ">>".cyan().bold(),
            s.name.bold(),
            s.kind.yellow(),
            s.file.dimmed(),
            s.line_start
        );
        if let Some(ref sig) = s.signature {
            if !sig.is_empty() {
                println!("  {}", sig.dimmed());
            }
        }

        // Show callers/callees
        let callers = q.callers(&s.name);
        if !callers.is_empty() {
            println!("  {} ({}):", "Callers".green(), callers.len());
            for c in &callers {
                println!("    {} [{}] {}", c.name, c.edge_kind.dimmed(), c.file.dimmed());
            }
        }
        let callees = q.callees(&s.name);
        if !callees.is_empty() {
            println!("  {} ({}):", "Callees".green(), callees.len());
            for c in &callees {
                println!("    {} [{}] {}", c.name, c.edge_kind.dimmed(), c.file.dimmed());
            }
        }
    }
}

pub fn fn_detail(name: &str, slug: Option<&str>, no_tests: bool) {
    let slug = require_slug(slug);
    let db = open_db(&slug);
    let q = GraphQueries::new(&db);

    let symbols = q.find_symbol(name);
    if symbols.is_empty() {
        println!("{} No symbol found: {}", "!".yellow(), name);
        return;
    }

    for s in &symbols {
        if no_tests && (s.file.contains("test") || s.file.contains("spec")) {
            continue;
        }

        println!(
            "{} {} [{}] in {}:{}-{}",
            ">>".cyan().bold(),
            s.name.bold(),
            s.kind.yellow(),
            s.file.dimmed(),
            s.line_start,
            s.line_end
        );
        if let Some(ref sig) = s.signature {
            if !sig.is_empty() {
                println!("  sig: {}", sig);
            }
        }
        if let Some(ref doc) = s.doc {
            if !doc.is_empty() {
                println!("  doc: {}", doc.dimmed());
            }
        }
        if let Some(ref content) = s.content {
            if !content.is_empty() {
                println!("{}", content);
            }
        }
    }
}

pub fn deps(file: &str, slug: Option<&str>) {
    let slug = require_slug(slug);
    let db = open_db(&slug);
    let q = GraphQueries::new(&db);

    let deps = q.file_deps(file);
    let rev_deps = q.file_reverse_deps(file);

    if deps.is_empty() && rev_deps.is_empty() {
        println!("{} No dependencies found for {}", "!".yellow(), file);
        return;
    }

    if !deps.is_empty() {
        println!("{} {} depends on:", ">>".cyan().bold(), file.bold());
        for d in &deps {
            println!("  {} [{}] x{}", d.file, d.kind.dimmed(), d.count);
        }
    }

    if !rev_deps.is_empty() {
        println!("\n{} Depended on by:", "<<".cyan().bold());
        for d in &rev_deps {
            println!("  {} [{}] x{}", d.file, d.kind.dimmed(), d.count);
        }
    }
}

pub fn impact(file: &str, slug: Option<&str>, depth: usize) {
    let slug = require_slug(slug);
    let db = open_db(&slug);
    let q = GraphQueries::new(&db);

    let impacted = q.impact(file, depth);
    if impacted.is_empty() {
        println!("{} No impact found for {}", "!".yellow(), file);
        return;
    }

    println!(
        "{} {} files impacted by changes to {}:",
        ">>".cyan().bold(),
        impacted.len().to_string().green(),
        file.bold()
    );
    for f in &impacted {
        println!("  {f}");
    }
}

pub fn fn_impact(name: &str, slug: Option<&str>, depth: usize) {
    let slug = require_slug(slug);
    let db = open_db(&slug);
    let q = GraphQueries::new(&db);

    let symbols = q.find_symbol(name);
    if symbols.is_empty() {
        println!("{} No symbol found: {}", "!".yellow(), name);
        return;
    }

    let mut all_impacted = std::collections::HashSet::new();
    for s in &symbols {
        let impacted = q.impact(&s.file, depth);
        for f in impacted {
            all_impacted.insert(f);
        }
    }

    if all_impacted.is_empty() {
        println!("{} No impact found for symbol {}", "!".yellow(), name);
        return;
    }

    println!(
        "{} {} files impacted by changes to symbol {}:",
        ">>".cyan().bold(),
        all_impacted.len().to_string().green(),
        name.bold()
    );
    for f in &all_impacted {
        println!("  {f}");
    }
}

pub fn map(slug: Option<&str>) {
    let slug = require_slug(slug);
    let db = open_db(&slug);
    let q = GraphQueries::new(&db);

    let stats = q.stats();
    let langs = q.language_breakdown();
    let top = q.top_connected(20);

    println!("{} Graph map for {}", ">>".cyan().bold(), slug.cyan());
    println!(
        "  {} symbols | {} edges | {} files",
        stats.symbols.to_string().green(),
        stats.edges.to_string().green(),
        stats.files.to_string().green()
    );

    if !langs.is_empty() {
        println!("\n  {}:", "Languages".bold());
        for l in &langs {
            println!("    {} ({})", l.language, l.count);
        }
    }

    if !top.is_empty() {
        println!("\n  {}:", "Top connected files".bold());
        for t in &top {
            println!("    {} ({})", t.file, t.connections);
        }
    }
}

pub fn cycles(slug: Option<&str>) {
    let slug = require_slug(slug);
    let db = open_db(&slug);
    let q = GraphQueries::new(&db);

    let cycles = q.detect_cycles();
    if cycles.is_empty() {
        println!("{} No cycles detected", "OK".green().bold());
        return;
    }

    println!(
        "{} {} circular dependencies detected:",
        "!".yellow().bold(),
        cycles.len().to_string().red()
    );
    for c in &cycles {
        println!("  {} <-> {}", c.from_file.red(), c.to_file.red());
    }
}

fn require_slug(slug: Option<&str>) -> String {
    match resolve_project_slug(&[slug]) {
        Some(s) => s,
        None => {
            eprintln!(
                "{} No project specified and none could be resolved",
                "Error:".red().bold()
            );
            std::process::exit(1);
        }
    }
}

use crate::config::Registry;
use crate::paths;
use crate::resolve::resolve_project_slug;
use colored::Colorize;
use graphmind_memory::cross_infer::infer_cross_links;
use graphmind_memory::cross_links::{CrossLinkStore, LinkType, NewCrossLink};

fn get_store() -> CrossLinkStore {
    let path = paths::cross_links_path();
    CrossLinkStore::new(&path)
}

pub fn cross_query(slug: Option<&str>) {
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

    let store = get_store();
    let links = store.find_by_project(&slug);

    if links.is_empty() {
        println!("{} No cross-links found for {}", "!".yellow(), slug);
        return;
    }

    println!(
        "{} {} cross-link(s) for {}:\n",
        ">>".cyan().bold(),
        links.len().to_string().green(),
        slug.cyan()
    );

    for l in &links {
        let type_str = serde_json::to_string(&l.link_type)
            .unwrap_or_default()
            .trim_matches('"')
            .to_string();
        println!(
            "  {} -> {} [{}]",
            l.from.cyan(),
            l.to.cyan(),
            type_str.yellow()
        );
        println!("    {}", l.reason.dimmed());
    }
}

pub fn cross_deps(slug: Option<&str>) {
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

    let store = get_store();
    let links = store.find_by_project(&slug);

    let outgoing: Vec<_> = links.iter().filter(|l| l.from == slug).collect();
    let incoming: Vec<_> = links.iter().filter(|l| l.to == slug).collect();

    if outgoing.is_empty() && incoming.is_empty() {
        println!("{} No cross-project dependencies for {}", "!".yellow(), slug);
        return;
    }

    if !outgoing.is_empty() {
        println!("{} {} depends on:", ">>".cyan().bold(), slug.cyan().bold());
        for l in &outgoing {
            println!("  {} ({})", l.to, l.reason.dimmed());
        }
    }

    if !incoming.is_empty() {
        println!(
            "\n{} Depended on by:",
            "<<".cyan().bold()
        );
        for l in &incoming {
            println!("  {} ({})", l.from, l.reason.dimmed());
        }
    }
}

pub fn cross_links() {
    let store = get_store();
    let links = store.list();

    if links.is_empty() {
        println!("{}", "No cross-links found.".dimmed());
        return;
    }

    println!(
        "{} {} cross-link(s):\n",
        ">>".cyan().bold(),
        links.len().to_string().green()
    );

    for l in &links {
        let type_str = serde_json::to_string(&l.link_type)
            .unwrap_or_default()
            .trim_matches('"')
            .to_string();
        let inferred_tag = if l.inferred { " (inferred)" } else { "" };
        println!(
            "  {} -> {} [{}]{} confidence={:.1}",
            l.from.cyan(),
            l.to.cyan(),
            type_str.yellow(),
            inferred_tag.dimmed(),
            l.confidence
        );
        println!("    {}", l.reason.dimmed());
    }
}

pub fn cross_link_add(from: &str, to: &str, link_type: &str, reason: &str) {
    let lt = match link_type {
        "depends-on" => LinkType::DependsOn,
        "extends" => LinkType::Extends,
        "uses-types-from" => LinkType::UsesTypesFrom,
        _ => LinkType::SharesPattern,
    };

    let store = get_store();
    let link = store.add(NewCrossLink {
        from: from.to_string(),
        to: to.to_string(),
        link_type: lt,
        reason: reason.to_string(),
        symbols: Vec::new(),
        inferred: false,
        confidence: 1.0,
    });

    println!(
        "{} Cross-link added: {} -> {} ({})",
        "OK".green().bold(),
        link.from.cyan(),
        link.to.cyan(),
        link.id.dimmed()
    );
}

pub fn cross_link_infer() {
    let projects = Registry::list();
    if projects.len() < 2 {
        println!("{} Need at least 2 registered projects to infer cross-links", "!".yellow());
        return;
    }

    let slugs: Vec<String> = projects.iter().map(|p| p.slug.clone()).collect();
    let store = get_store();

    let new_links = infer_cross_links(
        &slugs,
        |slug| paths::graph_db_path(slug).to_string_lossy().to_string(),
        &store,
    );

    if new_links.is_empty() {
        println!("{} No new cross-links inferred", "!".yellow());
    } else {
        println!(
            "{} {} new cross-link(s) inferred:",
            "OK".green().bold(),
            new_links.len().to_string().green()
        );
        for l in &new_links {
            println!("  {} -> {} ({})", l.from.cyan(), l.to.cyan(), l.reason.dimmed());
        }
    }
}

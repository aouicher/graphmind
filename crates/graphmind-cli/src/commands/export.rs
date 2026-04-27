use crate::paths;
use crate::resolve::resolve_project_slug;
use colored::Colorize;
use graphmind_db::queries::GraphQueries;
use graphmind_db::schema::init_database;
use graphmind_memory::cross_links::CrossLinkStore;

pub fn export(
    slug: Option<&str>,
    format: &str,
    cross: bool,
    obsidian: Option<&str>,
) {
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

    if let Some(vault_path) = obsidian {
        export_obsidian(&db, &slug, vault_path);
        return;
    }

    match format {
        "dot" => export_dot(&db, cross, &slug),
        "mermaid" => export_mermaid(&db, cross, &slug),
        "json" => export_json(&db, cross, &slug),
        _ => {
            eprintln!(
                "{} Unknown format: {}. Use dot, mermaid, or json.",
                "Error:".red().bold(),
                format
            );
            std::process::exit(1);
        }
    }
}

fn export_dot(db: &rusqlite::Connection, cross: bool, slug: &str) {
    let q = GraphQueries::new(db);
    let stats = q.stats();

    println!("digraph graphmind {{");
    println!("  rankdir=LR;");
    println!("  label=\"{slug} ({} symbols, {} edges)\";", stats.symbols, stats.edges);
    println!();

    // Nodes
    let mut stmt = db
        .prepare("SELECT id, name, kind, file FROM symbols")
        .unwrap();
    let symbols: Vec<(i64, String, String, String)> = stmt
        .query_map([], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
            ))
        })
        .unwrap()
        .filter_map(|r| r.ok())
        .collect();

    for (id, name, kind, file) in &symbols {
        let shape = match kind.as_str() {
            "Class" => "box",
            "Interface" => "diamond",
            "Function" | "Method" => "ellipse",
            _ => "note",
        };
        println!("  n{id} [label=\"{name}\" shape={shape} tooltip=\"{file}\"];");
    }

    println!();

    // Edges
    let mut stmt = db
        .prepare("SELECT from_id, to_id, kind FROM edges")
        .unwrap();
    let edges: Vec<(i64, i64, String)> = stmt
        .query_map([], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, String>(2)?,
            ))
        })
        .unwrap()
        .filter_map(|r| r.ok())
        .collect();

    for (from_id, to_id, kind) in &edges {
        let style = if kind == "imports" {
            "dashed"
        } else {
            "solid"
        };
        println!("  n{from_id} -> n{to_id} [label=\"{kind}\" style={style}];");
    }

    // Cross-links
    if cross {
        print_cross_links_dot(slug);
    }

    println!("}}");
}

fn export_mermaid(db: &rusqlite::Connection, cross: bool, slug: &str) {
    println!("graph LR");

    let mut stmt = db
        .prepare("SELECT id, name, kind FROM symbols")
        .unwrap();
    let symbols: Vec<(i64, String, String)> = stmt
        .query_map([], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
            ))
        })
        .unwrap()
        .filter_map(|r| r.ok())
        .collect();

    for (id, name, kind) in &symbols {
        let bracket = match kind.as_str() {
            "Class" => format!("[{name}]"),
            "Interface" => format!("{{{name}}}"),
            _ => format!("({name})"),
        };
        println!("  n{id}{bracket}");
    }

    let mut stmt = db
        .prepare("SELECT from_id, to_id, kind FROM edges")
        .unwrap();
    let edges: Vec<(i64, i64, String)> = stmt
        .query_map([], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, String>(2)?,
            ))
        })
        .unwrap()
        .filter_map(|r| r.ok())
        .collect();

    for (from_id, to_id, kind) in &edges {
        let arrow = if kind == "imports" {
            "-.->|imports|"
        } else {
            "-->|calls|"
        };
        println!("  n{from_id} {arrow} n{to_id}");
    }

    if cross {
        print_cross_links_mermaid(slug);
    }
}

fn export_json(db: &rusqlite::Connection, cross: bool, slug: &str) {
    let q = GraphQueries::new(db);
    let stats = q.stats();
    let langs = q.language_breakdown();

    let mut symbols_arr = Vec::new();
    let mut stmt = db
        .prepare("SELECT id, name, kind, file, line_start, line_end FROM symbols")
        .unwrap();
    let symbols: Vec<(i64, String, String, String, i64, i64)> = stmt
        .query_map([], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, i64>(4)?,
                row.get::<_, i64>(5)?,
            ))
        })
        .unwrap()
        .filter_map(|r| r.ok())
        .collect();

    for (id, name, kind, file, line_start, line_end) in &symbols {
        symbols_arr.push(serde_json::json!({
            "id": id,
            "name": name,
            "kind": kind,
            "file": file,
            "line_start": line_start,
            "line_end": line_end,
        }));
    }

    let mut edges_arr = Vec::new();
    let mut stmt = db
        .prepare("SELECT from_id, to_id, kind, file FROM edges")
        .unwrap();
    let edges: Vec<(i64, i64, String, Option<String>)> = stmt
        .query_map([], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, Option<String>>(3)?,
            ))
        })
        .unwrap()
        .filter_map(|r| r.ok())
        .collect();

    for (from_id, to_id, kind, file) in &edges {
        edges_arr.push(serde_json::json!({
            "from_id": from_id,
            "to_id": to_id,
            "kind": kind,
            "file": file,
        }));
    }

    let mut output = serde_json::json!({
        "slug": slug,
        "stats": {
            "symbols": stats.symbols,
            "edges": stats.edges,
            "files": stats.files,
        },
        "languages": langs.iter().map(|l| serde_json::json!({
            "language": l.language,
            "count": l.count,
        })).collect::<Vec<_>>(),
        "symbols": symbols_arr,
        "edges": edges_arr,
    });

    if cross {
        let store = CrossLinkStore::new(&paths::cross_links_path());
        let links = store.find_by_project(slug);
        let links_json: Vec<serde_json::Value> = links
            .iter()
            .map(|l| {
                serde_json::json!({
                    "from": l.from,
                    "to": l.to,
                    "type": serde_json::to_string(&l.link_type).unwrap_or_default().trim_matches('"'),
                    "reason": l.reason,
                })
            })
            .collect();
        output["cross_links"] = serde_json::json!(links_json);
    }

    println!("{}", serde_json::to_string_pretty(&output).unwrap_or_default());
}

fn export_obsidian(db: &rusqlite::Connection, slug: &str, vault_path: &str) {
    use std::io::Write;

    let vault = std::path::Path::new(vault_path);
    let graphmind_dir = vault.join("graphmind").join(slug);
    std::fs::create_dir_all(&graphmind_dir).unwrap_or_else(|e| {
        eprintln!("{} Failed to create vault directory: {}", "Error:".red().bold(), e);
        std::process::exit(1);
    });

    let mut stmt = db
        .prepare("SELECT id, name, kind, file, line_start FROM symbols")
        .unwrap();
    let symbols: Vec<(i64, String, String, String, i64)> = stmt
        .query_map([], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, i64>(4)?,
            ))
        })
        .unwrap()
        .filter_map(|r| r.ok())
        .collect();

    let mut stmt = db
        .prepare("SELECT e.from_id, s1.name, e.to_id, s2.name, e.kind FROM edges e JOIN symbols s1 ON s1.id = e.from_id JOIN symbols s2 ON s2.id = e.to_id")
        .unwrap();
    let edges: Vec<(i64, String, i64, String, String)> = stmt
        .query_map([], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, i64>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
            ))
        })
        .unwrap()
        .filter_map(|r| r.ok())
        .collect();

    let mut by_file: std::collections::HashMap<String, Vec<(i64, String, String, i64)>> =
        std::collections::HashMap::new();
    for (id, name, kind, file, line) in &symbols {
        by_file
            .entry(file.clone())
            .or_default()
            .push((*id, name.clone(), kind.clone(), *line));
    }

    let mut file_count = 0;
    for (file, syms) in &by_file {
        let safe_name = file.replace(['/', '\\'], "_");
        let md_path = graphmind_dir.join(format!("{safe_name}.md"));

        let mut content = String::new();
        content.push_str(&format!("# {file}\n\n"));

        for (id, name, kind, line) in syms {
            content.push_str(&format!("## {name} ({kind}) L{line}\n"));
            let related: Vec<_> = edges
                .iter()
                .filter(|(from_id, _, to_id, _, _)| from_id == id || to_id == id)
                .collect();
            for (_, from_name, _, to_name, edge_kind) in &related {
                content.push_str(&format!("- [[{from_name}]] --{edge_kind}--> [[{to_name}]]\n"));
            }
            content.push('\n');
        }

        let mut f = std::fs::File::create(&md_path).unwrap_or_else(|e| {
            eprintln!("{} Failed to write {}: {}", "Error:".red().bold(), md_path.display(), e);
            std::process::exit(1);
        });
        f.write_all(content.as_bytes()).ok();
        file_count += 1;
    }

    println!(
        "{} Exported {} files to {}",
        "OK".green().bold(),
        file_count,
        graphmind_dir.display()
    );
}

fn print_cross_links_dot(slug: &str) {
    let store = CrossLinkStore::new(&paths::cross_links_path());
    let links = store.find_by_project(slug);
    for l in &links {
        let type_str = serde_json::to_string(&l.link_type)
            .unwrap_or_default()
            .trim_matches('"')
            .to_string();
        println!(
            "  \"{}\" -> \"{}\" [label=\"{type_str}\" style=bold color=blue];",
            l.from, l.to
        );
    }
}

fn print_cross_links_mermaid(slug: &str) {
    let store = CrossLinkStore::new(&paths::cross_links_path());
    let links = store.find_by_project(slug);
    for l in &links {
        let type_str = serde_json::to_string(&l.link_type)
            .unwrap_or_default()
            .trim_matches('"')
            .to_string();
        println!("  {} ==>|{type_str}| {}", l.from, l.to);
    }
}

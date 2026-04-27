use crate::config::Registry;
use crate::paths;
use crate::resolve::resolve_project_slug;
use colored::Colorize;
use graphmind_db::queries::GraphQueries;
use graphmind_db::schema::init_database;

const MARKER_START: &str = "<!-- graphmind:start -->";
const MARKER_END: &str = "<!-- graphmind:end -->";

pub fn sync(slug: Option<&str>, all: bool, dir: Option<&str>) {
    if all {
        let projects = Registry::list();
        if projects.is_empty() {
            println!("{}", "No projects registered.".dimmed());
            return;
        }
        for p in &projects {
            sync_single(&p.slug, dir);
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

    sync_single(&slug, dir);
}

fn sync_single(slug: &str, dir: Option<&str>) {
    let project = match Registry::get(slug) {
        Some(p) => p,
        None => {
            eprintln!("{} Project {} not found", "Error:".red().bold(), slug);
            return;
        }
    };

    let target_dir = dir
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| std::path::PathBuf::from(&project.path));

    let claude_md_path = target_dir.join("CLAUDE.md");

    // Build the graphmind section
    let section = build_section(slug);

    let existing = std::fs::read_to_string(&claude_md_path).unwrap_or_default();

    let new_content = if existing.contains(MARKER_START) && existing.contains(MARKER_END) {
        // Replace between markers
        let start_idx = existing.find(MARKER_START).unwrap();
        let end_idx = existing.find(MARKER_END).unwrap() + MARKER_END.len();
        format!(
            "{}{}{}",
            &existing[..start_idx],
            section,
            &existing[end_idx..]
        )
    } else if existing.is_empty() {
        section
    } else {
        format!("{existing}\n{section}")
    };

    if let Err(e) = std::fs::write(&claude_md_path, new_content) {
        eprintln!(
            "{} Failed to write CLAUDE.md for {}: {}",
            "Error:".red().bold(),
            slug,
            e
        );
        return;
    }
    println!(
        "{} Synced CLAUDE.md for {} at {}",
        "OK".green().bold(),
        slug.cyan(),
        claude_md_path.display().to_string().dimmed()
    );
}

fn build_section(slug: &str) -> String {
    let mut lines = Vec::new();
    lines.push(MARKER_START.to_string());
    lines.push("\n## graphmind\n".to_string());

    let db_path = paths::graph_db_path(slug);
    if db_path.exists() {
        let db_path_str = db_path.to_string_lossy().to_string();
        if let Ok(db) = init_database(&db_path_str) {
            let q = GraphQueries::new(&db);
            let stats = q.stats();
            let langs = q.language_breakdown();

            let now = chrono::Utc::now().format("%Y-%m-%d").to_string();
            let lang_str: Vec<String> = langs
                .iter()
                .map(|l| format!("{} ({})", l.language, l.count))
                .collect();

            lines.push(format!(
                "Last build: {} | {} symbols | {} edges | {} files",
                now, stats.symbols, stats.edges, stats.files
            ));
            if !lang_str.is_empty() {
                lines.push(format!("Languages: {}", lang_str.join(", ")));
            }
            lines.push("MCP: `graphmind mcp` (stdio)".to_string());

            lines.push(String::new());
            lines.push("### Before editing anything".to_string());
            lines.push("- Symbol: `graphmind fn <symbol> --no-tests`".to_string());
            lines.push("- File: `graphmind deps <file>`".to_string());
            lines.push("- Git changes: `graphmind diff-impact`".to_string());
            lines.push("- Find by intent: `graphmind search \"handle auth; validate token\"`".to_string());

            lines.push(String::new());
            lines.push("### Rebuild when".to_string());
            lines.push("Structural changes, new modules, after merge.".to_string());
            lines.push("Command: `graphmind build`".to_string());
        } else {
            lines.push("Graph not built yet. Run `graphmind build`.".to_string());
        }
    } else {
        lines.push("Graph not built yet. Run `graphmind build`.".to_string());
    }

    lines.push(MARKER_END.to_string());
    lines.join("\n")
}

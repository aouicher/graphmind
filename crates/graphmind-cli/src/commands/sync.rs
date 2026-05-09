use graphmind_config::{paths, Registry, resolve_project_slug};
use colored::Colorize;
use graphmind_db::queries::GraphQueries;
use graphmind_db::schema::init_database;

const MARKER_START: &str = "<!-- GM:START -->";
const MARKER_END: &str = "<!-- GM:END -->";

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

    let section = build_section(slug);

    let existing = std::fs::read_to_string(&claude_md_path).unwrap_or_default();

    // Also handle old graphmind:start marker for migration
    let existing = if existing.contains("<!-- graphmind:start -->") {
        existing
            .replace("<!-- graphmind:start -->", MARKER_START)
            .replace("<!-- graphmind:end -->", MARKER_END)
    } else {
        existing
    };

    let new_content = if existing.contains(MARKER_START) && existing.contains(MARKER_END) {
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
    let version = env!("CARGO_PKG_VERSION");

    // Build project-specific stats header
    let stats_block = build_stats_block(slug);

    format!(
        r#"{MARKER_START}
<!-- GM:VERSION:{version} -->

## Mandatory: Code exploration via graphmind

In any graphmind-registered project, ALWAYS use `/gm` as the FIRST step for ANY code exploration, architecture understanding, or symbol lookup. This applies to:
- Answering questions about how something works
- Finding symbols, callers, dependencies, outlines
- Understanding code before making changes
- Exploring architecture or blast radius

**Do NOT** spawn Explore agents, use grep/find/Glob for code structure, or delegate code search to sub-agents until `/gm` has been tried first.

To **read** a symbol's source code, use `/gm-fn <symbol>` — it returns full source + callers + callees in one call. Prefer it over Read when you already know the symbol name.

Only fall back to grep for: string literals, config values, env vars, non-code patterns.

Invoke: `Skill(skill: "graphmind", args: "<your query>")`

**Auto-memory — mandatory behavior**: You MUST proactively save knowledge to graphmind memory **during** and after every session, without waiting for the user to ask. Save immediately when something is decided or discovered — do not wait for the end of the session.

Categories and commands:
- Architectural decisions → `graphmind memory add "<decision>" --type decision`
- Patterns/conventions → `--type pattern` or `--type convention`
- Bugs found or fixed → `--type bug`
- Critical always-needed facts → add `--priority` (injected every session)
- Cross-project or user-level facts → add `--global`

In a registered project: save without `--global` by default (project-scoped).
Outside a registered project: always use `--global`.

Be selective — only facts useful in a future session. Skip task details and temporary state.

## graphmind

{stats_block}
| Need | MCP tool | CLI equivalent |
|------|----------|----------------|
| Find symbol | `gm_fn` | `graphmind fn <name>` |
| Search by intent | `gm_search` | `graphmind search "<query>"` |
| File dependencies | `gm_deps` | `graphmind deps <file>` |
| Symbol resolution | `gm_query` | `graphmind query <name>` |
| Blast radius | `gm_fn_impact` | `graphmind fn-impact <name>` |
| Git diff impact | `gm_diff_impact` | `graphmind diff-impact` |
| Project overview | `gm_map` | `graphmind map` |
| File outline | `gm_outline` | `graphmind outline <file>` |
| Raw file content | `gm_file` | `graphmind file <file>` |
| Who calls chain | `gm_who_calls_chain` | — |
| Dead code | `gm_dead_code` | — |
| Cross-project | `gm_cross_query` | `graphmind cross query` |
| Memory search | `gm_memory_search` | `graphmind memory search` |

Only fall back to grep/find for: string literals, config values, non-code patterns.

### Rebuild when
Structural changes, new modules, after merge.
Command: `graphmind build`
{MARKER_END}"#
    )
}

fn build_stats_block(slug: &str) -> String {
    let db_path = paths::graph_db_path(slug);
    if !db_path.exists() {
        return "Graph not built yet. Run `graphmind build`.\n\n".to_string();
    }
    let db_path_str = db_path.to_string_lossy().to_string();
    let Ok(db) = init_database(&db_path_str) else {
        return "Graph not built yet. Run `graphmind build`.\n\n".to_string();
    };
    let q = GraphQueries::new(&db);
    let stats = q.stats();
    let langs = q.language_breakdown();
    let now = chrono::Utc::now().format("%Y-%m-%d").to_string();
    let lang_str: Vec<String> = langs
        .iter()
        .map(|l| format!("{} ({})", l.language, l.count))
        .collect();

    let mut out = format!(
        "Last build: {} | {} symbols | {} edges | {} files\n",
        now, stats.symbols, stats.edges, stats.files
    );
    if !lang_str.is_empty() {
        out.push_str(&format!("Languages: {}\n", lang_str.join(", ")));
    }
    out.push_str("MCP: `graphmind mcp` (stdio)\n\n");
    out
}

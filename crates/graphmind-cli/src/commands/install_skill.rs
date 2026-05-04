use colored::Colorize;
use std::fs;

const SKILL_CONTENT: &str = r#"---
description: >
  Code intelligence via graphmind. Use /gm <query> for ALL code exploration:
  find symbols, trace callers, view dependencies, file outlines, impact analysis.
  Replaces grep/find/Explore agents in any graphmind-registered project.
allowed-tools: Bash, Read
---

# /gm — Code Intelligence Query

Invoke with: `/gm <natural language query>`

## How to use

Describe what you need in plain language. The skill routes to the right graphmind command automatically.

Examples:
- `/gm where is dispatch_tool defined` → finds the symbol
- `/gm what calls dispatch_tool` → shows callers
- `/gm outline of server.rs` → file structure
- `/gm what depends on handlers.rs` → dependency map
- `/gm impact of changing build_graph` → blast radius
- `/gm project overview` → top connected files

## Routing logic

When you receive a `/gm` query, route as follows:

| Intent | Command |
|--------|---------|
| Find/search/where is | `graphmind search "<query>" --limit 15` |
| Callers/callees/who calls/what calls | `graphmind fn <symbol>` |
| Outline/structure/what's in file | `graphmind outline <file>` |
| Dependencies/imports/what depends on | `graphmind deps <file>` |
| Impact/blast radius/what breaks | `graphmind fn-impact <symbol>` |
| Project overview/map/architecture | `graphmind map` |
| Memory/decisions/conventions | `graphmind memory search "<query>"` |
| Git changes impact | `graphmind diff-impact` |

If ambiguous, default to `graphmind search`.

## Execution

1. Parse the query to determine intent and extract the target (symbol name, file path, or keywords)
2. Run the appropriate `graphmind` CLI command via Bash
3. Return the results directly

## When NOT to use /gm

- String literals, config values, env vars → use grep
- Editing files → use Read/Edit directly
- Running tests/builds → not a graphmind concern
"#;

const SUB_SKILLS: &[(&str, &str)] = &[
    ("gm-fn", "---\ndescription: >\n  Deep dive into a symbol: full source code + callers + callees.\n  Use when you need the call graph for debugging, refactoring, or impact analysis.\nallowed-tools: Bash\n---\n\n# /gm-fn — Symbol deep dive\n\nUsage: `/gm-fn <symbol> [file]`\n\nRuns `graphmind fn <symbol>` to get full source code + callers + callees.\n\nPass file path as second argument to disambiguate common names:\n- `/gm-fn create src/services/user.ts`\n\n## Execution\n\nRun via Bash: `graphmind fn <symbol>` (add `--file <path>` if file provided)\n"),
    ("gm-deps", "---\ndescription: >\n  File dependency map: what a file imports and what imports it.\n  Use to understand coupling before refactoring.\nallowed-tools: Bash\n---\n\n# /gm-deps — File dependencies\n\nUsage: `/gm-deps <file>`\n\nRuns `graphmind deps <file>` to show imports and dependents.\n\n## Execution\n\nRun via Bash: `graphmind deps <file>`\n"),
    ("gm-outline", "---\ndescription: >\n  File structure overview: classes, methods, functions with line numbers.\n  Use to understand a file before reading or editing it.\nallowed-tools: Bash\n---\n\n# /gm-outline — File outline\n\nUsage: `/gm-outline <file>`\n\nRuns `graphmind outline <file>` to show hierarchical symbol tree.\n\n## Execution\n\nRun via Bash: `graphmind outline <file>`\n"),
    ("gm-impact", "---\ndescription: >\n  Blast radius of a function change: all transitive callers.\n  Use before modifying a function to know what breaks.\nallowed-tools: Bash\n---\n\n# /gm-impact — Blast radius\n\nUsage: `/gm-impact <symbol>`\n\nRuns `graphmind fn-impact <symbol>` to find all callers.\n\n## Execution\n\nRun via Bash: `graphmind fn-impact <symbol>`\n"),
    ("gm-map", "---\ndescription: >\n  Project overview: top connected files ranked by importance.\n  Use to understand project architecture at a glance.\nallowed-tools: Bash\n---\n\n# /gm-map — Project map\n\nUsage: `/gm-map`\n\nRuns `graphmind map` to show the most connected files.\n\n## Execution\n\nRun via Bash: `graphmind map`\n"),
    ("gm-memory", "---\ndescription: >\n  Search or save to graphmind persistent memory (decisions, patterns, conventions).\n  Use to recall context or save important facts across sessions.\nallowed-tools: Bash\n---\n\n# /gm-memory — Persistent memory\n\nUsage:\n- `/gm-memory search <query>` — recall relevant facts\n- `/gm-memory add <fact>` — save a decision/pattern/convention\n- `/gm-memory add --priority <fact>` — save as always-injected\n\n## Execution\n\nRun via Bash:\n- Search: `graphmind memory search \"<query>\"`\n- Add: `graphmind memory add \"<content>\"`\n- Add priority: `graphmind memory add --priority \"<content>\"`\n"),
];

pub fn install_skill() {
    let skills_base = dirs::home_dir()
        .expect("Could not find home directory")
        .join(".claude")
        .join("skills");

    // Main skill
    let skill_dir = skills_base.join("graphmind");
    fs::create_dir_all(&skill_dir).unwrap_or_else(|e| {
        eprintln!("{} Failed to create skill directory: {}", "Error:".red().bold(), e);
        std::process::exit(1);
    });

    let skill_path = skill_dir.join("SKILL.md");
    fs::write(&skill_path, SKILL_CONTENT).unwrap_or_else(|e| {
        eprintln!("{} Failed to write SKILL.md: {}", "Error:".red().bold(), e);
        std::process::exit(1);
    });

    println!(
        "{} /gm skill → {}",
        "OK".green().bold(),
        skill_path.display()
    );

    // Sub-skills
    for (name, content) in SUB_SKILLS {
        let dir = skills_base.join(name);
        fs::create_dir_all(&dir).ok();
        fs::write(dir.join("SKILL.md"), content).unwrap_or_else(|e| {
            eprintln!("{} Failed to write {}/SKILL.md: {}", "Error:".red().bold(), name, e);
        });
        println!("    {} /{}", "OK".green().bold(), name);
    }
}

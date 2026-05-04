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

pub fn install_skill() {
    let skill_dir = dirs::home_dir()
        .expect("Could not find home directory")
        .join(".claude")
        .join("skills")
        .join("graphmind");

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
        "{} Skill installed to {}",
        "OK".green().bold(),
        skill_path.display()
    );
}

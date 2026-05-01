use colored::Colorize;
use std::fs;

const SKILL_CONTENT: &str = r#"---
description: >
  graphmind gives Claude Code persistent memory and structural code intelligence
  across all registered projects. Auto-triggers in any registered project directory.
  MANDATORY: query graphmind before using grep, find, or reading source files.
allowed-tools: Bash, Read, Write
---

# graphmind — Persistent Code Intelligence

## MANDATORY: Search through graphmind FIRST

**Before using Grep, find, rg, ag, or reading files to understand code:**
You MUST query graphmind first. Only fall back to grep/find if graphmind cannot answer (e.g., string literals, config values, non-code patterns).

| Need | Command | MCP tool |
|------|---------|----------|
| Find a symbol | `graphmind fn <symbol>` | `gm_fn` |
| Search by intent | `graphmind search "<query>"` | `gm_search` |
| File dependencies | `graphmind deps <file>` | `gm_deps` |
| Who calls X | `graphmind query <symbol>` | `gm_query` |
| Blast radius | `graphmind fn-impact <symbol>` | `gm_fn_impact` |
| Git change impact | `graphmind diff-impact` | `gm_diff_impact` |
| Project overview | `graphmind map` | `gm_map` |
| Cross-project | `graphmind cross query <symbol>` | `gm_cross_query` |

## Is this project registered?
Check: `graphmind status`
Register if not: `graphmind register .`

## The 3-Layer Rule — always follow this order

### Layer 1 — Structural graph (what the code IS)
Query before touching any code. This is faster and more accurate than grep.

### Layer 2 — Semantic memory (what was DECIDED)
Query for context, decisions, conventions:
- `graphmind memory search "<query>"` or MCP `gm_memory_search`

### Layer 3 — Raw files (LAST RESORT)
Read source files only when editing or when layers 1-2 cannot answer.
Use grep/find ONLY for: string literals, config values, non-code patterns, regex searches.

## When to rebuild the graph
- After adding/removing modules or major refactors
- After `git merge` with structural changes
- NOT every session — the graph persists between sessions
- Command: `graphmind build` (fast, SHA256-cached)

## NEVER
- NEVER grep/find for symbols, functions, or imports — graphmind has this indexed
- NEVER re-read the entire codebase if graphmind can answer it
- NEVER manually edit files in `~/.graphmind/`
- NEVER rebuild the graph every session
- NEVER skip the 3-layer rule
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

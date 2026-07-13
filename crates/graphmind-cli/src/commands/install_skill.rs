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
- `/gm where is dispatch_tool defined` → search
- `/gm what calls dispatch_tool` → fn (callers/callees)
- `/gm how do we reach handle_request` → who-calls (transitive)
- `/gm outline of server.rs` → file structure
- `/gm what depends on handlers.rs` → deps
- `/gm impact of changing build_graph` → blast radius
- `/gm project overview` → map
- `/gm dead code` → unused functions
- `/gm circular deps` → cycles
- `/gm export dispatch_tool as mermaid` → graph export
- `/gm listeners for order_created` → event handlers

## Routing logic

When you receive a `/gm` query, route as follows:

| Intent | Command |
|--------|---------|
| Find/search/where is | `graphmind search "<query>" --limit 15` |
| Full source + callers/callees | `graphmind fn <symbol>` |
| Compact callers/callees (no source) | `graphmind query <symbol>` |
| Outline/structure/what's in file | `graphmind outline <file>` |
| Raw file content | `graphmind file <file>` |
| Dependencies/imports/what depends on | `graphmind deps <file>` |
| Impact/blast radius/what breaks | `graphmind fn-impact <symbol>` |
| How do we reach this function | `graphmind who-calls <symbol>` |
| Project overview/map/architecture | `graphmind map` |
| Memory/decisions/conventions | `graphmind memory search "<query>"` |
| Git changes impact | `graphmind diff-impact` |
| Cross-project search | `graphmind cross query <symbol>` |
| Dead code / unused functions | `graphmind dead-code` |
| Circular dependencies | `graphmind cycles` |
| Similar/duplicate functions | `graphmind similar <symbol>` |
| Export graph (Mermaid/DOT) | `graphmind export --symbol <name>` |
| Event listeners | `graphmind listeners <event>` |
| Project status/health | `graphmind status` |
| Rebuild index | `graphmind build` |
| Raw file content | `graphmind file <file>` |

If ambiguous, default to `graphmind search`.

## Execution

1. Parse the query to determine intent and extract the target (symbol name, file path, or keywords)
2. Run the appropriate `graphmind` CLI command via Bash:
   ```
   graphmind <command>
   ```
3. Return the results directly

## When NOT to use /gm

- String literals, config values, env vars → use grep
- Editing files → use Read/Edit directly
- Running tests/builds → not a graphmind concern
"#;

const SUB_SKILLS: &[(&str, &str)] = &[
    ("gm-search", "---\ndescription: >\n  Search symbols by name or keyword. Auto-expands callers/callees when exactly 1 result.\n  Default entry point for finding code.\nallowed-tools: Bash\n---\n\n# /gm-search — Find symbols\n\nUsage: `/gm-search <query>`\n\nRuns `graphmind search \"<query>\" --limit 15` to find symbols by name, keyword, or intent.\n\nSupports multi-query with `;`: `/gm-search dispatch; handler`\n\n## Execution\n\nRun via Bash: `graphmind search \"<query>\" --limit 15`\n"),
    ("gm-fn", "---\ndescription: >\n  Deep dive into any symbol: full source code + callers + callees.\n  Primary tool for understanding code — use whenever you need to read a function,\n  see its relationships, or explore how it fits in the codebase.\nallowed-tools: Bash\n---\n\n# /gm-fn — Symbol deep dive\n\nUsage: `/gm-fn <symbol> [file]`\n\nRuns `graphmind fn <symbol>` to get full source code + callers + callees.\n\nPass file path as second argument to disambiguate common names:\n- `/gm-fn create src/services/user.ts`\n\n## Execution\n\nRun via Bash: `graphmind fn <symbol>` (add `--file <path>` if file provided)\n"),
    ("gm-query", "---\ndescription: >\n  Find a symbol and its callers/callees with compact snippets.\n  Like gm-fn but returns snippets instead of full source.\nallowed-tools: Bash\n---\n\n# /gm-query — Symbol query (compact)\n\nUsage: `/gm-query <symbol>`\n\nRuns `graphmind query <symbol>` to find a symbol with compact caller/callee snippets.\n\n## Execution\n\nRun via Bash: `graphmind query <symbol>`\n"),
    ("gm-deps", "---\ndescription: >\n  File dependency map: what a file imports and what imports it.\n  Use to understand a file's connections, navigate the codebase, or assess coupling.\nallowed-tools: Bash\n---\n\n# /gm-deps — File dependencies\n\nUsage: `/gm-deps <file>`\n\nRuns `graphmind deps <file>` to show imports and dependents.\n\n## Execution\n\nRun via Bash: `graphmind deps <file>`\n"),
    ("gm-outline", "---\ndescription: >\n  File structure overview: classes, methods, functions with line numbers.\n  Use to navigate, explore, or get a quick picture of what a file contains.\nallowed-tools: Bash\n---\n\n# /gm-outline — File outline\n\nUsage: `/gm-outline <file>`\n\nRuns `graphmind outline <file>` to show hierarchical symbol tree.\n\n## Execution\n\nRun via Bash: `graphmind outline <file>`\n"),
    ("gm-impact", "---\ndescription: >\n  Blast radius of a function: all transitive callers.\n  Use to assess importance, plan changes, or understand how widely a symbol is used.\nallowed-tools: Bash\n---\n\n# /gm-impact — Blast radius\n\nUsage: `/gm-impact <symbol>`\n\nRuns `graphmind fn-impact <symbol>` to find all callers.\n\n## Execution\n\nRun via Bash: `graphmind fn-impact <symbol>`\n"),
    ("gm-who-calls", "---\ndescription: >\n  Trace transitive callers of a symbol up to N depth (BFS).\n  Answers \"how do we reach this function?\" across the call graph.\nallowed-tools: Bash\n---\n\n# /gm-who-calls — Transitive caller chain\n\nUsage: `/gm-who-calls <symbol> [depth]`\n\nRuns `graphmind who-calls <symbol>` to trace all paths leading to a function.\n\nDefault depth: 3. Pass a number to increase: `/gm-who-calls handle_request 5`\n\n## Execution\n\nRun via Bash: `graphmind who-calls <symbol> --depth <N>`\n"),
    ("gm-map", "---\ndescription: >\n  Project overview: top connected files ranked by importance.\n  Use to understand project architecture at a glance.\nallowed-tools: Bash\n---\n\n# /gm-map — Project map\n\nUsage: `/gm-map`\n\nRuns `graphmind map` to show the most connected files.\n\n## Execution\n\nRun via Bash: `graphmind map`\n"),
    ("gm-memory", "---\ndescription: >\n  Search or save to graphmind persistent memory (decisions, patterns, conventions).\n  Use to recall context or save important facts across sessions.\nallowed-tools: Bash\n---\n\n# /gm-memory — Persistent memory\n\nUsage:\n- `/gm-memory search <query>` — recall relevant facts\n- `/gm-memory add <fact>` — save a decision/pattern/convention\n- `/gm-memory add --priority <fact>` — save as always-injected\n\n## Execution\n\nRun via Bash:\n- Search: `graphmind memory search \"<query>\"`\n- Add: `graphmind memory add \"<content>\"`\n- Add priority: `graphmind memory add --priority \"<content>\"`\n"),
    ("gm-diff", "---\ndescription: >\n  Analyze git diff impact on the code graph. Shows which symbols and files\n  are affected by current uncommitted changes.\nallowed-tools: Bash\n---\n\n# /gm-diff — Git diff impact\n\nUsage: `/gm-diff`\n\nRuns `graphmind diff-impact` to show symbols affected by current git changes.\n\n## Execution\n\nRun via Bash: `graphmind diff-impact`\n"),
    ("gm-cross", "---\ndescription: >\n  Cross-project search: find symbols across all registered graphmind projects.\n  Use when a symbol might be defined in another repo.\nallowed-tools: Bash\n---\n\n# /gm-cross — Cross-project search\n\nUsage: `/gm-cross <symbol>`\n\nRuns `graphmind cross query <symbol>` to search across all projects.\n\n## Execution\n\nRun via Bash: `graphmind cross query <symbol>`\n"),
    ("gm-dead-code", "---\ndescription: >\n  Find symbols with no incoming edges — likely dead code candidates.\n  Use to audit, clean up, or understand which parts of the codebase are unused.\nallowed-tools: Bash\n---\n\n# /gm-dead-code — Dead code detection\n\nUsage: `/gm-dead-code`\n\nRuns `graphmind dead-code` to find functions/methods with no callers.\n\n## Execution\n\nRun via Bash: `graphmind dead-code`\n"),
    ("gm-cycles", "---\ndescription: >\n  Detect circular dependencies between files.\n  Use to understand architecture, spot coupling, or validate module boundaries.\nallowed-tools: Bash\n---\n\n# /gm-cycles — Circular dependencies\n\nUsage: `/gm-cycles`\n\nRuns `graphmind cycles` to detect circular file dependencies.\n\n## Execution\n\nRun via Bash: `graphmind cycles`\n"),
    ("gm-similar", "---\ndescription: >\n  Find structurally similar symbols (same kind, similar size and callee count).\n  Use to spot duplication, find related patterns, or discover analogous implementations.\nallowed-tools: Bash\n---\n\n# /gm-similar — Find similar symbols\n\nUsage: `/gm-similar <symbol>`\n\nRuns `graphmind similar <symbol>` to find structurally similar functions.\n\n## Execution\n\nRun via Bash: `graphmind similar <symbol>`\n"),
    ("gm-export", "---\ndescription: >\n  Export a file or symbol neighborhood as Mermaid flowchart or DOT graph.\n  Use for documentation or visual architecture understanding.\nallowed-tools: Bash\n---\n\n# /gm-export — Export graph visualization\n\nUsage:\n- `/gm-export file <path>` — export file subgraph\n- `/gm-export symbol <name>` — export symbol neighborhood\n\nOutputs Mermaid by default. Add `--format dot` for DOT format.\n\n## Execution\n\nRun via Bash: `graphmind export --file <path>` or `graphmind export --symbol <name>`\n"),
    ("gm-listeners", "---\ndescription: >\n  Find all functions/methods that listen to a domain event.\n  Use to trace event-driven architectures and understand side effects.\nallowed-tools: Bash\n---\n\n# /gm-listeners — Event listeners\n\nUsage: `/gm-listeners <event_name>`\n\nRuns `graphmind listeners <event>` to find all handlers for a domain event.\n\nExample: `/gm-listeners order_created`\n\n## Execution\n\nRun via Bash: `graphmind listeners <event>`\n"),
    ("gm-status", "---\ndescription: >\n  Health check: graph stats, symbol/edge count, last build time, languages.\n  Use to verify a project is indexed and up to date.\nallowed-tools: Bash\n---\n\n# /gm-status — Project status\n\nUsage: `/gm-status`\n\nRuns `graphmind status` to show graph stats for the current project.\n\n## Execution\n\nRun via Bash: `graphmind status`\n"),
    ("gm-build", "---\ndescription: >\n  Rebuild the code graph index. Use after adding/removing modules,\n  major refactors, or after git merge with structural changes.\nallowed-tools: Bash\n---\n\n# /gm-build — Rebuild graph\n\nUsage: `/gm-build`\n\nRuns `graphmind build` to reindex the project (fast, SHA256-cached).\n\n## Execution\n\nRun via Bash: `graphmind build`\n"),
    ("gm-file", "---\ndescription: >\n  Read raw source content of a file from a registered project.\n  Use when you need the full file rather than individual symbols.\nallowed-tools: Bash\n---\n\n# /gm-file — Raw file content\n\nUsage: `/gm-file <file>`\n\nRuns `graphmind file <file>` to read a file by path relative to the project root.\n\n## Execution\n\nRun via Bash: `graphmind file <file>`\n"),
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

    // Sub-skills
    let mut installed = 0;
    let mut failed = Vec::new();
    for (name, content) in SUB_SKILLS {
        let dir = skills_base.join(name);
        fs::create_dir_all(&dir).ok();
        match fs::write(dir.join("SKILL.md"), content) {
            Ok(_) => installed += 1,
            Err(e) => failed.push(format!("{name}: {e}")),
        }
    }

    println!(
        "    {} /gm + {} sub-skills installed ({})",
        "✓".green().bold(),
        installed,
        skills_base.display()
    );

    if !failed.is_empty() {
        for f in &failed {
            eprintln!("    {} {}", "✗".red(), f);
        }
    }
}

/// Remove the /gm skill and all sub-skills installed by `install_skill()`.
/// Only touches `~/.claude/skills/graphmind` and the exact sub-skill dirs
/// from `SUB_SKILLS` — leaves any other directory under `~/.claude/skills/` untouched.
pub fn uninstall_skill() {
    let skills_base = dirs::home_dir()
        .expect("Could not find home directory")
        .join(".claude")
        .join("skills");

    let mut removed = 0;

    let skill_dir = skills_base.join("graphmind");
    if skill_dir.exists() {
        fs::remove_dir_all(&skill_dir).ok();
        removed += 1;
    }

    for (name, _) in SUB_SKILLS {
        let dir = skills_base.join(name);
        if dir.exists() {
            fs::remove_dir_all(&dir).ok();
            removed += 1;
        }
    }

    println!(
        "    {} removed {} skill dir(s) ({})",
        "✓".green().bold(),
        removed,
        skills_base.display()
    );
}

use colored::Colorize;
use graphmind_config::paths::home_dir;
use std::fs;

/// The main skill. It lives in `~/.claude/skills/graphmind/` (older installs, uninstall and the
/// desktop look there), so `name: gm` is what makes Claude Code list it as `/gm`; a skill's name
/// otherwise defaults to its directory name, which would make it `/graphmind`.
const SKILL_CONTENT: &str = r#"---
name: gm
description: >
  Code intelligence via graphmind. Use /gm <query> for ALL code exploration:
  find symbols, trace callers, view dependencies, file outlines, impact analysis.
  Replaces grep/find/Explore agents in any graphmind-registered project, and
  answers questions about a registered project from outside it (with --slug).
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
- `/gm export the graph as mermaid` → graph export
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
| Export graph (Mermaid/DOT) | `graphmind export --format mermaid` |
| Event listeners | `graphmind listeners <event>` |
| Project status/health | `graphmind status` |
| Rebuild index | `graphmind build` |

If ambiguous, default to `graphmind search`.

## Outside an indexed project

Commands default to the project containing the current directory. Check where you are with `graphmind which` before the first command: inside a registered project it prints the project's slug (exit code 0); outside every registered project it prints nothing and exits with code 1. Don't wait for a command to fail: outside a project, `graphmind memory search` without `--slug` exits 0 and searches global memory only.

Outside a registered project:

1. Run `graphmind list` to see the registered projects (slug and path).
2. Pick the project the question is about. If the question is about the current folder and no registered project matches, use your normal tools (and suggest `graphmind init` to index the folder). Ask the user only when several registered projects could match.
3. Pass it to every command with `--slug <project>`, for example:
   - `graphmind search "retry policy" --limit 15 --slug myapp`
   - `graphmind fn dispatch_tool --slug myapp`
   - `graphmind memory search "retry policy" --slug myapp`

Every command above takes `--slug` except `graphmind list` and `graphmind cross query`, which span all projects. A slug that isn't registered fails with "Project '<slug>' is not registered (see `graphmind list`)". `graphmind diff-impact` reads the git changes of the current directory, so it only works from inside the project's checkout.

When a registered project matches the question, never fall back to grep or find across unrelated folders.

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
    ("gm-diff", "---\ndescription: >\n  Analyze git diff impact on the code graph. Shows which symbols and files\n  are affected by current uncommitted changes.\nallowed-tools: Bash\n---\n\n# /gm-diff — Git diff impact\n\nUsage: `/gm-diff`\n\nRuns `graphmind diff-impact` to show symbols affected by current git changes.\n\nIt reads the git changes of the current directory, so run it from inside the project's checkout.\n\n## Execution\n\nRun via Bash: `graphmind diff-impact`\n"),
    ("gm-cross", "---\ndescription: >\n  Cross-project search: find symbols across all registered graphmind projects.\n  Use when a symbol might be defined in another repo.\nallowed-tools: Bash\n---\n\n# /gm-cross — Cross-project search\n\nUsage: `/gm-cross <symbol>`\n\nRuns `graphmind cross query <symbol>` to search across all projects.\n\n## Execution\n\nRun via Bash: `graphmind cross query <symbol>`\n"),
    ("gm-dead-code", "---\ndescription: >\n  Find symbols with no incoming edges — likely dead code candidates.\n  Use to audit, clean up, or understand which parts of the codebase are unused.\nallowed-tools: Bash\n---\n\n# /gm-dead-code — Dead code detection\n\nUsage: `/gm-dead-code`\n\nRuns `graphmind dead-code` to find functions/methods with no callers.\n\n## Execution\n\nRun via Bash: `graphmind dead-code`\n"),
    ("gm-cycles", "---\ndescription: >\n  Detect circular dependencies between files.\n  Use to understand architecture, spot coupling, or validate module boundaries.\nallowed-tools: Bash\n---\n\n# /gm-cycles — Circular dependencies\n\nUsage: `/gm-cycles`\n\nRuns `graphmind cycles` to detect circular file dependencies.\n\n## Execution\n\nRun via Bash: `graphmind cycles`\n"),
    ("gm-similar", "---\ndescription: >\n  Find structurally similar symbols (same kind, similar size and callee count).\n  Use to spot duplication, find related patterns, or discover analogous implementations.\nallowed-tools: Bash\n---\n\n# /gm-similar — Find similar symbols\n\nUsage: `/gm-similar <symbol>`\n\nRuns `graphmind similar <symbol>` to find structurally similar functions.\n\n## Execution\n\nRun via Bash: `graphmind similar <symbol>`\n"),
    ("gm-export", "---\ndescription: >\n  Export the project graph as a Mermaid flowchart, DOT graph or JSON.\n  Use for documentation or visual architecture understanding.\nallowed-tools: Bash\n---\n\n# /gm-export — Export graph visualization\n\nUsage: `/gm-export [mermaid|dot|json]`\n\nExports the whole project graph (default format: JSON). Add `--cross` to include cross-project links.\n\n## Execution\n\nRun via Bash: `graphmind export --format <mermaid|dot|json>`\n"),
    ("gm-listeners", "---\ndescription: >\n  Find all functions/methods that listen to a domain event.\n  Use to trace event-driven architectures and understand side effects.\nallowed-tools: Bash\n---\n\n# /gm-listeners — Event listeners\n\nUsage: `/gm-listeners <event_name>`\n\nRuns `graphmind listeners <event>` to find all handlers for a domain event.\n\nExample: `/gm-listeners order_created`\n\n## Execution\n\nRun via Bash: `graphmind listeners <event>`\n"),
    ("gm-status", "---\ndescription: >\n  Health check: graph stats, symbol/edge count, last build time, languages.\n  Use to verify a project is indexed and up to date.\nallowed-tools: Bash\n---\n\n# /gm-status — Project status\n\nUsage: `/gm-status`\n\nRuns `graphmind status` to show graph stats for the current project.\n\n## Execution\n\nRun via Bash: `graphmind status`\n"),
    ("gm-build", "---\ndescription: >\n  Rebuild the code graph index. Use after adding/removing modules,\n  major refactors, or after git merge with structural changes.\nallowed-tools: Bash\n---\n\n# /gm-build — Rebuild graph\n\nUsage: `/gm-build`\n\nRuns `graphmind build` to reindex the project (fast, SHA256-cached).\n\n## Execution\n\nRun via Bash: `graphmind build`\n"),
    ("gm-file", "---\ndescription: >\n  Read raw source content of an indexed file from a registered project.\n  Use when you need the full file rather than individual symbols.\nallowed-tools: Bash\n---\n\n# /gm-file — Raw file content\n\nUsage: `/gm-file <file>`\n\nRuns `graphmind file <file>` to read an indexed file (one the graph covers) by path relative to the project root.\n\n## Execution\n\nRun via Bash: `graphmind file <file>`\n"),
];

/// Appended to every sub-skill whose command takes `--slug`: all but those in `SUB_SKILLS_WITHOUT_PROJECT_NOTE`.
const SUB_SKILL_PROJECT_NOTE: &str = "\n## Outside an indexed project\n\nIf `graphmind which` prints nothing (exit code 1), you are outside every registered project: run `graphmind list`, pick the project the question is about (ask the user only when several could match) and add `--slug <project>` to the command.\n";

/// Appended to gm-memory's note: memory commands don't fail outside a project.
const MEMORY_PROJECT_NOTE: &str = "\nOutside a project, `graphmind memory search` without `--slug` exits 0 and searches global memory only. Save a fact about a project with `--slug <project>`, and use `--global` only for cross-project or personal facts.\n";

/// gm-cross searches every project; gm-diff reads the current directory's git changes.
const SUB_SKILLS_WITHOUT_PROJECT_NOTE: &[&str] = &["gm-cross", "gm-diff"];

/// The SKILL.md text of a sub-skill, with the project note where it applies.
fn sub_skill_text(name: &str, content: &str) -> String {
    if SUB_SKILLS_WITHOUT_PROJECT_NOTE.contains(&name) {
        content.to_string()
    } else if name == "gm-memory" {
        format!("{content}{SUB_SKILL_PROJECT_NOTE}{MEMORY_PROJECT_NOTE}")
    } else {
        format!("{content}{SUB_SKILL_PROJECT_NOTE}")
    }
}

pub fn install_skill() {
    let skills_base = home_dir()
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
        match fs::write(dir.join("SKILL.md"), sub_skill_text(name, content)) {
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
///
/// Counts only the directories actually removed. A directory that can't be removed is reported
/// on stderr with its path and the reason, and makes the call return `Err` (a directory that
/// doesn't exist is not a failure).
pub fn uninstall_skill() -> Result<(), String> {
    let skills_base = home_dir()
        .join(".claude")
        .join("skills");

    let dirs = std::iter::once("graphmind").chain(SUB_SKILLS.iter().map(|(name, _)| *name));
    let mut removed = 0;
    let mut failed = 0;
    for name in dirs {
        let dir = skills_base.join(name);
        if !dir.exists() {
            continue;
        }
        match fs::remove_dir_all(&dir) {
            Ok(()) => removed += 1,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => {
                failed += 1;
                eprintln!("    {} could not remove {}: {}", "✗".red(), dir.display(), e);
            }
        }
    }

    if removed > 0 {
        println!(
            "    {} removed {} skill dir(s) ({})",
            "✓".green().bold(),
            removed,
            skills_base.display()
        );
    } else if failed == 0 {
        println!(
            "    {} no graphmind skills installed ({})",
            "✓".green().bold(),
            skills_base.display()
        );
    }

    if failed > 0 {
        Err(format!("failed to remove {failed} skill dir(s) under {}", skills_base.display()))
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frontmatter(skill: &str) -> &str {
        let body = skill.strip_prefix("---\n").expect("frontmatter");
        &body[..body.find("\n---\n").expect("closing ---")]
    }

    /// Claude Code takes a skill's name from `name:`, else from its directory (`graphmind`), so
    /// without it `/gm` doesn't exist even though every message tells people to use it.
    #[test]
    fn main_skill_is_named_gm() {
        assert!(frontmatter(SKILL_CONTENT).lines().any(|l| l.trim() == "name: gm"));
    }

    /// Skill selection reads the description: it must cover questions asked from outside a project.
    #[test]
    fn main_skill_description_covers_use_outside_a_project() {
        let description = frontmatter(SKILL_CONTENT);
        assert!(description.contains("from outside it (with --slug)"), "{description}");
    }

    /// Outside a project the agent checks `graphmind which` up front (memory search doesn't fail),
    /// uses its normal tools for an unregistered folder, and targets a project with `--slug`.
    #[test]
    fn main_skill_explains_use_outside_a_project() {
        let start = SKILL_CONTENT.find("## Outside an indexed project").expect("section");
        let end = SKILL_CONTENT.find("## Execution").expect("next section");
        let section = &SKILL_CONTENT[start..end];
        for expected in [
            "`graphmind which`",
            "prints nothing and exits with code 1",
            "`graphmind memory search` without `--slug` exits 0 and searches global memory only",
            "`graphmind list`",
            "use your normal tools",
            "Ask the user only when several registered projects could match",
            "--slug <project>",
            "except `graphmind list` and `graphmind cross query`",
            "is not registered (see `graphmind list`)",
        ] {
            assert!(section.contains(expected), "missing {expected:?}:\n{section}");
        }
        assert!(!section.contains("No project specified"), "{section}");
    }

    #[test]
    fn routing_table_lists_each_intent_once() {
        let rows: Vec<&str> = SKILL_CONTENT.lines().filter(|l| l.starts_with("| ") && l.contains("`graphmind")).collect();
        let unique: std::collections::HashSet<&str> = rows.iter().copied().collect();
        assert_eq!(rows.len(), unique.len(), "{rows:#?}");
    }

    #[test]
    fn sub_skills_get_the_project_note_unless_they_span_projects() {
        for (name, content) in SUB_SKILLS {
            let text = sub_skill_text(name, content);
            let has_note = text.contains("`graphmind which`") && text.contains("--slug <project>") && text.contains("`graphmind list`");
            assert_eq!(has_note, !SUB_SKILLS_WITHOUT_PROJECT_NOTE.contains(name), "{name}");
        }
    }

    #[test]
    fn memory_skill_says_search_reads_only_global_memory_without_slug() {
        let (name, content) = SUB_SKILLS.iter().find(|(name, _)| *name == "gm-memory").expect("gm-memory");
        let text = sub_skill_text(name, content);
        assert!(text.contains("without `--slug` exits 0 and searches global memory only"), "{text}");
        assert!(text.contains("use `--global` only for cross-project or personal facts"), "{text}");
    }

    /// Sub-skills are invoked by their directory name (`/gm-fn`), which is what their text says.
    #[test]
    fn sub_skills_are_invoked_by_their_directory_name() {
        for (dir, content) in SUB_SKILLS {
            assert!(dir.starts_with("gm-"), "{dir}");
            assert!(content.contains(&format!("/{dir}")), "{dir} doesn't mention /{dir}");
        }
    }
}

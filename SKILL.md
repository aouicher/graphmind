---
description: >
  graphmind gives Claude Code persistent memory and structural code intelligence
  across all registered projects. Auto-triggers in any registered project directory.
  Use the cheapest tool for the question: graphmind for callers, blast radius and
  searches with no names to grep for; rg for exact strings.
allowed-tools: Bash, Read, Write
---

# graphmind — Persistent Code Intelligence

## When to use graphmind

Use the cheapest tool that answers the question; no tool has to come first.
1. "Why is it like this / did we already decide this": `graphmind memory search "<terms>"` and the repo's ADRs, before reading code.
2. Exact strings, config values, env vars, routes, migrations: `rg` (or Grep), then Read.
3. A vague "where is the thing that does X", with no names to search for: `graphmind search "<query>"`, then Read.
4. Callers, callees or blast radius of one symbol: `graphmind fn <symbol>` or `gm_who_calls_chain`, then Read what you'll edit.
5. Memory and code disagree: say so and show both.

| Need | Command | MCP tool |
|------|---------|----------|
| Find a symbol | `graphmind fn <symbol>` | `gm_fn` |
| Search by intent | `graphmind search "<query>"` | `gm_search` |
| File dependencies | `graphmind deps <file>` | `gm_deps` |
| Who calls X | `graphmind query <symbol>` | `gm_query` |
| Blast radius | `graphmind fn-impact <symbol>` | `gm_fn_impact` |
| Git change impact | `graphmind diff-impact` | `gm_diff_impact` |
| Project overview | `graphmind map` | `gm_map` |
| File outline | `graphmind outline <file>` | `gm_outline` |
| Raw file content | `graphmind file <file>` | `gm_file` |
| Who calls chain | — | `gm_who_calls_chain` |
| Dead code | — | `gm_dead_code` |
| Cross-project | `graphmind cross query <symbol>` | `gm_cross_query` |
| Memory search | `graphmind memory search "<query>"` | `gm_memory_search` |

## Is this project registered?
Check: `graphmind status`
Register if not: `graphmind register .`

## Graph — what the code IS
Use it for structural questions:
- Find a symbol: `graphmind fn <symbol>` or MCP `gm_fn`
- File dependencies: `graphmind deps <file>` or MCP `gm_deps`
- Blast radius before editing: `graphmind fn-impact <symbol> --no-tests`
- Impact of current git changes: `graphmind diff-impact`
- Entry points: `graphmind map`
- File structure: `graphmind outline <file>` or MCP `gm_outline`
- Transitive callers: MCP `gm_who_calls_chain`

## Memory — what was DECIDED, PREFERRED, KNOWN
Search it before reading code when the question is "why is it like this" or "did we already decide this":
- `graphmind memory search "<query>"` or MCP `gm_memory_search`

**Recall:** the prompt hook searches memory on each prompt and injects the facts that match. Use `gm_memory_search` for deeper or targeted recall.

**Save** when something is decided or discovered that a future session needs, with `graphmind memory add "<fact>" --type <type> --tags <tags>` or MCP `gm_memory_add`:
- **Decisions** (type: decision) — architecture choices, tech choices, trade-off resolutions
- **Patterns** (type: pattern) — recurring approaches, solutions, code patterns
- **Conventions** (type: convention) — naming, style, workflow rules
- **Bugs** (type: bug) — known issues, workarounds, gotchas
- **Context** (type: context) — business context, project goals, stakeholder info, user preferences

Save rules:
- Do NOT ask "should I save this?" — just save it
- Use `project=<slug>` for project-specific facts (or the absolute path of the checkout — in a git worktree, its path selects its own graph and the repo's shared memory)
- Use `global=true` for user preferences, team conventions, cross-project knowledge
- One fact per entry, under about 600 characters, with the exact identifiers someone would search for (file paths, function, config and command names)
- When a fact changes, rewrite it (`graphmind memory update <id> --content "<new text>"`) or delete it; don't add a contradicting fact next to it
- `--priority` only for the few facts every session needs
- Do NOT save ephemeral task progress or things derivable from code/git; a decision recorded in the repo (an ADR) needs no memory copy

## Files
Read a file when you are about to edit it, after `rg` finds an exact string, or when the graph and memory don't have the answer.

## Cross-project queries
When a symbol or pattern might exist in another project:
`graphmind cross query <symbol>` or MCP `gm_cross_query`

## Session workflow
**Start of session:** run `graphmind session start` → loads graph summary + recent memory
**End of session:** run `graphmind session save` → saves decisions and progress to memory

## When to rebuild the graph
- After adding/removing modules or major refactors
- After `git merge` with structural changes
- NOT every session — the graph persists between sessions
- Command: `graphmind build` (fast, SHA256-cached)

## NEVER
- Never re-read the entire codebase when one graph query or one `rg` answers the question
- Never manually edit files in `~/.graphmind/`
- Never rebuild the graph every session

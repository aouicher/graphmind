---
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
| File outline | `graphmind outline <file>` | `gm_outline` |
| Who calls chain | — | `gm_who_calls_chain` |
| Dead code | — | `gm_dead_code` |
| Cross-project | `graphmind cross query <symbol>` | `gm_cross_query` |
| Memory search | `graphmind memory search "<query>"` | `gm_memory_search` |

## Is this project registered?
Check: `graphmind status`
Register if not: `graphmind register .`

## The 3-Layer Rule — always follow this order

### Layer 1 — Structural graph (what the code IS)
Query before touching any code:
- Find a symbol: `graphmind fn <symbol>` or MCP `gm_fn`
- File dependencies: `graphmind deps <file>` or MCP `gm_deps`
- Blast radius before editing: `graphmind fn-impact <symbol> --no-tests`
- Impact of current git changes: `graphmind diff-impact`
- Entry points: `graphmind map`
- File structure: `graphmind outline <file>` or MCP `gm_outline`
- Transitive callers: MCP `gm_who_calls_chain`

### Layer 2 — Persistent memory (what was DECIDED, PREFERRED, KNOWN)
Query for context, decisions, conventions:
- `graphmind memory search "<query>"` or MCP `gm_memory_search`

**AUTO-RECALL:** The hook automatically searches memory on each prompt. Use `gm_memory_search` for deeper/targeted recall.

**AUTO-SAVE:** When the conversation reveals any of the following, immediately save via `gm_memory_add`:
- **Decisions** (type: decision) — architecture choices, tech choices, trade-off resolutions
- **Patterns** (type: pattern) — recurring approaches, solutions, code patterns
- **Conventions** (type: convention) — naming, style, workflow rules
- **Bugs** (type: bug) — known issues, workarounds, gotchas
- **Context** (type: context) — business context, project goals, stakeholder info, user preferences

Save rules:
- Do NOT ask "should I save this?" — just save it
- Use `project=<slug>` for project-specific facts
- Use `global=true` for user preferences, team conventions, cross-project knowledge
- Keep entries atomic: one fact per entry, clear and reusable
- Do NOT save ephemeral task progress or things derivable from code/git

### Layer 3 — Raw files (only when layers 1-2 are insufficient)
Read source files only when editing or when the graph doesn't have the answer.

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
- Never use grep/find/rg to search for symbols — use graphmind
- Never re-read the entire codebase if graphmind can answer it
- Never manually edit files in `~/.graphmind/`
- Never rebuild the graph every session
- Never skip the 3-layer rule

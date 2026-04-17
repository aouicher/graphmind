---
description: >
  graphmind gives Claude Code persistent memory and structural code intelligence
  across all registered projects. Auto-triggers in any registered project directory.
  Always query the graph before reading source files.
allowed-tools: Bash, Read, Write
---

# graphmind — Persistent Code Intelligence

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

### Layer 2 — Semantic memory (what was DECIDED)
Query for context, decisions, conventions:
- `graphmind memory search "<query>"` or MCP `gm_memory_search`

### Layer 3 — Raw files (only when layers 1-2 are insufficient)
Read source files only when editing or when the graph doesn't have the answer.

## Cross-project queries
When a symbol or pattern might exist in another project:
`graphmind cross-query <symbol>` or MCP `gm_cross_query`

## Session workflow
**Start of session:** run `graphmind session start` → loads graph summary + recent memory
**End of session:** run `graphmind session save` → saves decisions and progress to memory

## When to rebuild the graph
- After adding/removing modules or major refactors
- After `git merge` with structural changes
- NOT every session — the graph persists between sessions
- Command: `graphmind build` (fast, SHA256-cached)

## NEVER
- Never re-read the entire codebase if graphmind can answer it
- Never manually edit files in `~/.graphmind/`
- Never rebuild the graph every session
- Never skip the 3-layer rule

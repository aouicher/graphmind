<!-- GM:START -->
<!-- GM:VERSION:0.2.205 -->

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

Last build: 2026-06-13 | 59 symbols | 36 edges | 10 files
Languages: typescript (5), rust (1), ruby (1), python (1), markdown (1), go (1)
MCP: `graphmind mcp` (stdio)


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
<!-- GM:END -->
# graphmind

> Your codebase has memory. Use it.

Persistent, local-first code intelligence for Claude Code. Structural graph + semantic memory + cross-project links — all on your machine.

## The Problem

Every new Claude Code session starts from zero. Claude re-reads your entire codebase, re-discovers architecture, and forgets every decision you explained last time. Across multiple projects, there's zero visibility into shared dependencies.

**graphmind** fixes this with three layers:
1. **Structural graph** — function-level code knowledge graph per repo (AST-based, tree-sitter)
2. **Semantic memory** — declarative store for decisions, patterns, conventions
3. **Cross-project links** — relationships between registered repos

Everything is 100% local. No cloud. No open ports by default. No telemetry.

## Quick Start

```bash
npm install -g @graphmind/cli

graphmind register .          # register your project
graphmind build               # build the code graph
graphmind mcp                 # start MCP server for Claude Code
```

Add to `~/.claude.json`:
```json
{
  "mcpServers": {
    "graphmind": {
      "command": "graphmind",
      "args": ["mcp"]
    }
  }
}
```

## Architecture

```
┌─────────────────────────────────────────────┐
│  Claude Code / MCP Client                    │
├─────────────────────────────────────────────┤
│  MCP Server (stdio)                          │
│  gm_query · gm_fn · gm_deps · gm_impact    │
│  gm_memory_search · gm_memory_add           │
│  gm_cross_query · gm_diff_impact            │
│  gm_status · gm_context                     │
├─────────────────────────────────────────────┤
│  Layer 1: Structural Graph (SQLite)          │
│  Symbols · Edges · Call sites · FTS5         │
├─────────────────────────────────────────────┤
│  Layer 2: Semantic Memory (JSONL)            │
│  Decisions · Patterns · Conventions          │
├─────────────────────────────────────────────┤
│  Layer 3: Cross-Project Links (JSONL)        │
│  Shared symbols · Inferred relationships     │
├─────────────────────────────────────────────┤
│  Rust Core (tree-sitter + napi-rs)           │
│  TS/JS parsing · Symbol extraction · Rayon   │
└─────────────────────────────────────────────┘
```

## Commands

### Registry
```bash
graphmind register [path]     # register current dir
graphmind unregister <slug>   # remove project
graphmind list                # all projects
graphmind status              # health check
```

### Build
```bash
graphmind build [slug]        # incremental build
graphmind build --all         # all projects
graphmind build --full        # force full rebuild
graphmind build --watch       # watch mode (debounced 2s)
```

### Query
```bash
graphmind query <symbol>      # find symbol + connections
graphmind fn <symbol>         # call chain + callers
graphmind deps <file>         # file dependency map
graphmind impact <file>       # transitive reverse deps
graphmind fn-impact <symbol>  # blast radius
graphmind map [slug]          # most-connected files
graphmind cycles [slug]       # circular dependencies
```

### Memory
```bash
graphmind memory add "<fact>" [--project <slug>] [--global]
graphmind memory search "<query>"
graphmind memory list
graphmind memory delete <id>
```

### Search
```bash
graphmind search "<query>"          # semantic search (requires embed)
graphmind search "<q1>; <q2>"       # multi-query with RRF ranking
graphmind search "<query>" --kind function
graphmind embed [slug]              # build local embeddings (one-time)
```

### Export
```bash
graphmind export [slug] -f dot      # Graphviz dot format
graphmind export [slug] -f mermaid  # Mermaid diagram
graphmind export [slug] -f json     # JSON graph
graphmind export --cross -f mermaid # cross-project diagram
```

### Cross-Project
```bash
graphmind cross-query <symbol>      # search across ALL projects
graphmind cross-deps <slug>         # who depends on this project
graphmind cross-links               # all cross-project relationships
graphmind cross-link add <a> <b>    # manual link
graphmind cross-link infer          # auto-detect shared symbols
```

### Diff Impact
```bash
graphmind diff-impact               # impact of current git changes
graphmind diff-impact --staged      # staged changes only
graphmind diff-impact --depth 3     # limit trace depth
```

### Sessions
```bash
graphmind session start [slug]      # log session start
graphmind session save ["message"]  # save session summary
graphmind session history [slug]    # recent sessions
```

### Integration
```bash
graphmind mcp                 # start MCP server (stdio)
graphmind sync [slug]         # update CLAUDE.md
graphmind install-skill       # install Claude Code skill
```

## Claude Code Integration

### MCP Server
graphmind exposes tools via MCP (Model Context Protocol) over stdio:
- `gm_query` / `gm_fn` / `gm_deps` / `gm_impact` — graph queries
- `gm_memory_search` / `gm_memory_add` — memory operations
- `gm_cross_query` / `gm_cross_deps` / `gm_cross_links` — cross-project
- `gm_diff_impact` — git change impact analysis
- `gm_status` / `gm_context` — project metadata

### Skill
```bash
graphmind install-skill
```
Installs a Claude Code skill that teaches Claude the 3-layer rule: query the graph first, check memory second, read raw files only when needed.

### CLAUDE.md Sync
```bash
graphmind sync
```
Injects a section into your project's `CLAUDE.md` with graph stats, quick-reference commands, and related projects.

## Security

- **No open ports by default** — MCP uses stdio. HTTP transport binds to `127.0.0.1` only.
- **No API keys in config** — optional LLM uses `apiKeyCommand` (shell out to Keychain).
- **Path traversal protection** — all file ops restricted to registered paths + `~/.graphmind/`.
- **No network calls in core** — only `graphmind embed` (one-time model download) touches the network.
- **Atomic writes** — memory JSONL writes use tmp+rename to prevent corruption.
- **MCP write confirmation** — `gm_memory_add` requires explicit confirmation.

## Language Support

| Language | Extensions | Status |
|----------|-----------|--------|
| TypeScript | `.ts`, `.tsx` | Phase 1 |
| JavaScript | `.js`, `.jsx`, `.mjs` | Phase 1 |
| Python | `.py` | Phase 2 |
| Ruby | `.rb` | Phase 2 |
| Go | `.go` | Phase 2 |
| Rust | `.rs` | Phase 2 |

## Data Storage

All data lives in `~/.graphmind/`:
```
~/.graphmind/
├── config.json          # registered projects
├── memory/              # JSONL memory files
├── graphs/<slug>/       # SQLite graph databases
├── cross-links/         # cross-project relationships
└── sessions/            # daily session logs
```

Everything is plaintext or SQLite — fully inspectable with standard tools.

## Roadmap

- [x] Phase 1: Core MVP (graph + memory + CLI + MCP)
- [x] Phase 2: Cross-project, diff-impact, sessions
- [x] Phase 3: Semantic search (local embeddings, RRF multi-query)
- [x] Phase 4: Watch mode, export (dot/mermaid/json)
- [ ] Phase 5: Python/Ruby/Go support
- [ ] Phase 6: Obsidian vault export, git hooks

## Contributing

MIT License. Contributions welcome.

```bash
git clone https://github.com/aouicher/graphmind
cd graphmind
npm install
cargo build           # build Rust core
npm run build         # build TypeScript
npm test              # run tests
```

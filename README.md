# graphmind

[![CI](https://github.com/aouicher/graphmind/actions/workflows/ci.yml/badge.svg)](https://github.com/aouicher/graphmind/actions/workflows/ci.yml)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](https://opensource.org/licenses/MIT)

> Your codebase has memory. Use it.

Persistent, local-first code intelligence for Claude Code. Structural graph + semantic memory + cross-project links — all on your machine.

## The Problem

Every new Claude Code session starts from zero. Claude re-reads your entire codebase, re-discovers architecture, and forgets every decision you explained last time. Across multiple projects, there's zero visibility into shared dependencies.

**graphmind** fixes this with four layers:
1. **Structural graph** — function-level code knowledge graph per repo (AST-based, tree-sitter)
2. **Semantic embeddings** — vector search over symbols (local ONNX, OpenAI, or Voyage AI)
3. **Semantic memory** — declarative store for decisions, patterns, conventions
4. **Cross-project links** — relationships between registered repos

Everything is 100% local. No cloud. No open ports by default. No telemetry.

## Install

### Homebrew (macOS/Linux)

```bash
brew install aouicher/graphmind/graphmind
```

### Cargo

```bash
cargo install --git https://github.com/aouicher/graphmind graphmind-cli
```

### Shell script

```bash
curl -fsSL https://raw.githubusercontent.com/aouicher/graphmind/main/scripts/install.sh | bash
```

### From source

```bash
git clone https://github.com/aouicher/graphmind
cd graphmind
cargo build --release -p graphmind-cli
cp target/release/graphmind ~/.local/bin/
```

## Quick Start

```bash
graphmind register .          # register your project
graphmind build               # build the code graph
graphmind sync                # inject graph context into CLAUDE.md
graphmind mcp                 # start MCP server for Claude Code
```

## Setup with Claude Code

### 1. Global MCP server (recommended)

Add graphmind as a global MCP server so Claude Code can use it in every project:

```bash
claude mcp add graphmind -- graphmind mcp
```

Or manually in `~/.claude/settings.json`:
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

### Setup with Claude Desktop

Claude Desktop does not inherit your shell PATH. You **must** use the full path to the binary:

```json
{
  "mcpServers": {
    "graphmind": {
      "command": "/opt/homebrew/bin/graphmind",
      "args": ["mcp"]
    }
  }
}
```

Config file location:
- **macOS**: `~/Library/Application Support/Claude/claude_desktop_config.json`
- **Windows**: `%APPDATA%\Claude\claude_desktop_config.json`

> **Tip**: Run `which graphmind` to find the correct path on your system.

### Per-project `.mcp.json` (optional)

You can also add graphmind per-project by creating a `.mcp.json` at the root of your repo:
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

This is picked up automatically by Claude Code when you open the project.

### 2. Per-project CLAUDE.md (recommended)

`graphmind sync` injects a section into your project's `CLAUDE.md` with graph stats and quick-reference commands. Claude reads this automatically at the start of every session.

```bash
cd your-project
graphmind sync                # updates CLAUDE.md in current project
graphmind sync --all          # updates CLAUDE.md for all registered projects
```

This adds a block like:
```markdown
<!-- graphmind:start -->
## graphmind

Last build: 2026-04-17 | 142 symbols | 87 edges | 34 files
Languages: typescript (25), javascript (9)
MCP: `graphmind mcp` (stdio)

### Before editing anything
- Symbol: `graphmind fn <symbol> --no-tests`
- File: `graphmind deps <file>`
- Git changes: `graphmind diff-impact`
- Find by intent: `graphmind search "handle auth; validate token"`

### Rebuild when
Structural changes, new modules, after merge.
Command: `graphmind build`
<!-- graphmind:end -->
```

Re-run `graphmind sync` after each build to keep it current.

### 3. Claude Code search hook (recommended)

Installs a Claude Code hook that transparently rewrites `grep`/`find`/`rg` commands to `graphmind search`. Claude gets graph-powered results without changing its workflow.

```bash
graphmind install hook-claude
```

This registers hooks in `~/.claude/settings.json` for:
- **PreToolUse** — rewrites grep/find/rg to `graphmind search`, provides graph results for Grep/Glob/LS tools
- **SessionStart** — loads project context (stats, structure) at session start
- **UserPromptSubmit** — pre-fetches relevant graph context based on the user's prompt
- **PostToolUse** — enriches results with graph-aware suggestions

The hook automatically bypasses rewriting for exhaustive searches (e.g., "find all occurrences", `grep -c`, pipes to `wc`/`sort`).

To uninstall:
```bash
graphmind uninstall hook-claude
```

### 4. Claude Code skill (optional)

Installs a skill that teaches Claude the 3-layer rule: query the graph first, check memory second, read raw files only when needed.

```bash
graphmind install skill
```

### 5. Git hooks (optional)

Auto-rebuild on commit, impact check on push:

```bash
graphmind install hook-git
```

### Multi-project setup

Register multiple projects, then Claude can query across all of them:

```bash
cd ~/projects/api && graphmind register .
cd ~/projects/web && graphmind register .
cd ~/projects/shared-lib && graphmind register .

graphmind build --all
graphmind cross link infer    # auto-detect shared symbols
graphmind sync --all
```

## Architecture

```
┌─────────────────────────────────────────────┐
│  Claude Code / MCP Client                    │
├─────────────────────────────────────────────┤
│  MCP Server (rmcp SDK, stdio) — 24 tools      │
│  gm_query · gm_fn · gm_deps · gm_impact    │
│  gm_outline · gm_who_calls_chain · gm_dead  │
│  gm_export · gm_similar · gm_listeners      │
│  gm_memory_search · gm_cross_query          │
│  gm_status · gm_context · gm_diff_impact    │
├─────────────────────────────────────────────┤
│  Layer 1: Structural Graph (SQLite + FTS5)   │
│  Symbols · Edges · Call sites                │
├─────────────────────────────────────────────┤
│  Layer 2: Semantic Embeddings (SQLite)       │
│  Cosine search · RRF fusion · Multi-provider│
├─────────────────────────────────────────────┤
│  Layer 3: Semantic Memory (JSONL)            │
│  Decisions · Patterns · Conventions          │
├─────────────────────────────────────────────┤
│  Layer 4: Cross-Project Links (JSONL)        │
│  Shared symbols · Inferred relationships     │
├─────────────────────────────────────────────┤
│  Rust Core (tree-sitter + napi-rs)           │
│  Multi-language parsing · Symbol extraction  │
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
graphmind clean [slug]        # remove graph cache (forces full rebuild)
graphmind clean --all         # clean all projects
```

### Exclude
```bash
graphmind exclude list                    # show all patterns
graphmind exclude add grafana-data        # exclude from current project
graphmind exclude add grafana-data --global  # exclude from all projects
graphmind exclude remove grafana-data     # re-include
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
graphmind search "<query>"          # FTS search across symbols
graphmind search "<q1>; <q2>"       # multi-query with RRF ranking
graphmind search "<query>" --kind function
```

### Embeddings

Semantic vector search over symbols. Configured in `~/.graphmind/config.json`:

```json
{
  "embedding": {
    "mode": "voyage",
    "model": "voyage-code-3",
    "api_keys": {
      "voyage": "pa-..."
    }
  }
}
```

**Providers:**

| Mode | Model (default) | Notes |
|------|----------------|-------|
| `local` | `all-MiniLM-L6-v2` (384d) | ONNX, no API key needed |
| `openai` | `text-embedding-3-small` (1536d) | Supports custom `openai_base_url` |
| `voyage` | `voyage-code-3` (1024d) | Code-specialized, recommended |
| `disabled` | — | No embeddings (default) |

Embeddings are computed automatically during `graphmind build` when a provider is configured. If the model changes, the embedding index is rebuilt automatically.

OpenAI-compatible providers (Azure, proxys) can set a custom base URL:
```json
{
  "embedding": {
    "mode": "openai",
    "model": "text-embedding-3-large",
    "openai_base_url": "https://your-proxy.example.com/v1",
    "api_keys": { "openai": "sk-..." }
  }
}
```

### Export
```bash
graphmind export [slug] -f dot            # Graphviz dot format
graphmind export [slug] -f mermaid        # Mermaid diagram
graphmind export [slug] -f json           # JSON graph
graphmind export --cross -f mermaid       # cross-project diagram
graphmind export --obsidian ~/vault/      # Obsidian vault with [[wikilinks]]
```

### Cross-Project
```bash
graphmind cross query <symbol>      # search across ALL projects
graphmind cross deps <slug>         # who depends on this project
graphmind cross links               # all cross-project relationships
graphmind cross link add <a> <b>    # manual link
graphmind cross link infer          # auto-detect shared symbols
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

### Install / Uninstall
```bash
graphmind install hook-claude     # Claude Code search hook
graphmind install hook-git        # git hooks (post-commit + pre-push)
graphmind install skill           # Claude Code skill
graphmind uninstall hook-claude   # remove Claude Code hook
graphmind uninstall hook-git      # remove git hooks
graphmind sync [slug]             # inject graph context into CLAUDE.md
graphmind sync --all              # update CLAUDE.md for all projects
```

## MCP Tools Reference

graphmind exposes 24 tools via MCP (Model Context Protocol):

| Tool | Description |
|------|-------------|
| `gm_query` | Find symbol and its connections |
| `gm_fn` | Function call chain + callers |
| `gm_deps` | File-level dependency map |
| `gm_impact` | Transitive reverse dependencies |
| `gm_fn_impact` | Blast radius for a symbol |
| `gm_diff_impact` | Impact of current git changes |
| `gm_map` | Most-connected files |
| `gm_cycles` | Circular dependency detection |
| `gm_search` | Full-text search across symbols |
| `gm_listeners` | Find event listeners by event name |
| `gm_outline` | Hierarchical file structure with qualified names |
| `gm_who_calls_chain` | Transitive caller chain (BFS) |
| `gm_dead_code` | Find symbols with no incoming edges |
| `gm_export` | Export subgraph as Mermaid/DOT |
| `gm_similar` | Find structurally similar symbols |
| `gm_memory_search` | Search stored decisions/patterns |
| `gm_memory_add` | Store a fact (requires confirmation) |
| `gm_memory_list` | List memory entries |
| `gm_cross_query` | Symbol search across all projects |
| `gm_cross_deps` | Cross-project dependency graph |
| `gm_cross_links` | List all cross-project links |
| `gm_status` | Project health and stats |
| `gm_context` | Full project context for session start |
| `gm_list_projects` | All registered projects |

## Security

- **No open ports by default** — MCP uses stdio.
- **Path traversal protection** — all file ops restricted to registered paths + `~/.graphmind/`.
- **No network calls by default** — everything runs locally. Embedding API calls only when explicitly configured.
- **API keys stored locally** — in `~/.graphmind/config.json`, never sent anywhere except the configured provider.
- **Atomic writes** — memory JSONL writes use tmp+rename to prevent corruption.
- **MCP write confirmation** — `gm_memory_add` requires explicit confirmation.

## Language Support

| Language | Extensions | Status |
|----------|-----------|--------|
| TypeScript | `.ts`, `.tsx` | Stable |
| JavaScript | `.js`, `.jsx`, `.mjs` | Stable |
| Python | `.py` | Stable |
| Go | `.go` | Stable |
| Rust | `.rs` | Stable |
| Ruby | `.rb` | Stable |
| Terraform (HCL) | `.tf`, `.tfvars` | Stable |
| YAML | `.yml`, `.yaml` | Stable |
| Markdown | `.md` | Stable |
| C | `.c`, `.h` | Stable |
| Objective-C | `.m`, `.mm` | Stable |
| Java | `.java` | Stable |
| PHP | `.php` | Stable |
| Swift | `.swift` | Stable |
| Bash | `.sh`, `.bash`, `.zsh` | Stable |
| Perl | `.pl`, `.pm` | Stable |
| CSS | `.css` | Stable |
| SCSS | `.scss`, `.sass` | Stable |
| HTML | `.html`, `.htm` | Stable |
| TOML | `.toml` | Stable |
| Dockerfile | `Dockerfile` | Stable |
| SQL | `.sql` | Stable |
| C++ | `.cpp`, `.cc`, `.cxx`, `.hpp` | Stable |
| C# | `.cs` | Stable |
| Kotlin | `.kt`, `.kts` | Stable |
| Dart | `.dart` | Stable |
| Scala | `.scala`, `.sc` | Stable |
| R | `.r`, `.R` | Stable |
| GraphQL | `.graphql`, `.gql` | Stable |
| PowerShell | `.ps1`, `.psm1` | Stable |

## Data Storage

All data lives in `~/.graphmind/`:
```
~/.graphmind/
├── config.json          # registered projects + embedding settings
├── memory/              # JSONL memory files
├── graphs/<slug>/
│   ├── graph.db         # structural graph (SQLite + FTS5)
│   ├── embeddings.db    # vector embeddings (SQLite)
│   ├── meta.json        # build stats
│   └── cache/           # incremental build cache
├── cross-links/         # cross-project relationships
└── sessions/            # daily session logs
```

Everything is plaintext or SQLite — fully inspectable with standard tools.

## Contributing

MIT License. Contributions welcome.

```bash
git clone https://github.com/aouicher/graphmind
cd graphmind
cargo build --release -p graphmind-cli
cargo clippy --workspace -- -D warnings
cargo test --workspace
```

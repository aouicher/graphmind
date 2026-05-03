# graphmind

Local-first code intelligence CLI with MCP server.

## Tech Stack
- **Rust** — full Cargo workspace
- **Parsing**: tree-sitter via napi-rs (`crates/graphmind-core/`)
- **Graph**: SQLite + FTS5 (`crates/graphmind-db/`)
- **Memory**: JSONL store + cross-project inference (`crates/graphmind-memory/`)
- **Embeddings**: cosine search + RRF (`crates/graphmind-embeddings/`)
- **MCP**: rmcp SDK v1.5, stdio transport (`crates/graphmind-mcp/`)
- **CLI**: clap (`crates/graphmind-cli/`)

## Commands
```bash
cargo check --workspace       # check all crates
cargo build --release -p graphmind-cli  # build release binary
cargo clippy --workspace -- -D warnings # lint
cargo test --workspace        # run tests
```

## User-facing setup
```bash
graphmind setup               # global one-time: hooks + MCP + skill
graphmind init                # per-project: register + git hooks + build
graphmind init --skip-build   # per-project without building
```

## Project Structure
- `crates/graphmind-core/` — Rust parsing engine (tree-sitter, napi-rs)
- `crates/graphmind-db/` — SQLite graph (schema, builder, queries, FTS5, cache)
- `crates/graphmind-memory/` — JSONL memory store + cross-project links
- `crates/graphmind-embeddings/` — embedding store, cosine search, RRF fusion
- `crates/graphmind-mcp/` — MCP server (rmcp SDK, 24 tools)
- `crates/graphmind-cli/` — CLI (clap, all commands)

## Installation
```bash
curl -fsSL https://raw.githubusercontent.com/aouicher/graphmind/main/scripts/install.sh | bash
```

<!-- graphmind:start -->

## graphmind

Last build: 2026-05-01 | 934 symbols | 2246 edges | 131 files
Languages: rust (92), typescript (20), toml (9), markdown (3), bash (2), ruby (1), python (1), html (1), go (1), css (1)
MCP: `graphmind mcp` (stdio)

### MANDATORY: use graphmind for ALL code exploration
NEVER use grep/find/ls/Glob to search code. Use graphmind MCP tools:

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
| Who calls chain | `gm_who_calls_chain` | — |
| Dead code | `gm_dead_code` | — |
| Cross-project | `gm_cross_query` | `graphmind cross query` |
| Memory search | `gm_memory_search` | `graphmind memory search` |

Only fall back to grep/find for: string literals, config values, non-code patterns.

### Rebuild when
Structural changes, new modules, after merge.
Command: `graphmind build`
<!-- graphmind:end -->

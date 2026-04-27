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

## Project Structure
- `crates/graphmind-core/` — Rust parsing engine (tree-sitter, napi-rs)
- `crates/graphmind-db/` — SQLite graph (schema, builder, queries, FTS5, cache)
- `crates/graphmind-memory/` — JSONL memory store + cross-project links
- `crates/graphmind-embeddings/` — embedding store, cosine search, RRF fusion
- `crates/graphmind-mcp/` — MCP server (rmcp SDK, 18 tools)
- `crates/graphmind-cli/` — CLI (clap, all commands)

## Installation
```bash
curl -fsSL https://raw.githubusercontent.com/aouicher/graphmind/main/scripts/install.sh | bash
```

<!-- graphmind:start -->
## graphmind

MCP: `graphmind mcp` (stdio)

### Before editing anything
- Symbol: `graphmind fn <symbol> --no-tests`
- File: `graphmind deps <file>`
- Git changes: `graphmind diff-impact`
- Find by intent: `graphmind search "handle auth; validate token"`

### Rebuild when
Structural changes, new modules, after merge.
Command: `graphmind build --incremental`
<!-- graphmind:end -->

# graphmind

Local-first code intelligence CLI with MCP server.

## Tech Stack
- **Rust core**: tree-sitter parsing via napi-rs (crates/graphmind-core/)
- **TypeScript CLI**: commander + better-sqlite3 + @modelcontextprotocol/sdk
- **Build**: tsup | **Test**: vitest | **Lint**: biome

## Commands
```bash
npm run build        # build TypeScript
npm test             # run tests
npm run typecheck    # tsc --noEmit
npm run lint         # biome check
cargo check          # check Rust core
cargo build          # build Rust native addon
```

## Project Structure
- `crates/graphmind-core/` — Rust parsing engine (tree-sitter, napi-rs)
- `src/cli/` — CLI commands (commander)
- `src/core/graph/` — SQLite graph (schema, builder, queries, cache)
- `src/core/memory/` — JSONL memory store
- `src/mcp/` — MCP server + tools
- `tests/` — vitest test suite


<!-- graphmind:start -->
## graphmind

Last build: 2026-04-17 | 0 symbols | 0 edges | 4432 files
Languages: javascript (2333), typescript (2099)
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

# Changelog

User-facing changes per release. Each entry becomes the release notes on GitHub.
Releases before 0.3.0: see [GitHub releases](https://github.com/aouicher/graphmind/releases).

## [Unreleased]

### Fixed

- **Ignored files stay out of the graph.** Exclude patterns and `.gitignore` files now follow gitignore rules (`**`, `*`, root-anchored entries like `/build`, `!` negation, nested `.gitignore` files). Before, wildcard and root-anchored entries silently matched nothing, so ignored files (generated code, credentials, private scripts) were indexed and, with a remote embedding provider, sent to it. Thanks @KennethWKZ.

### Changed

- `.ignore` files in the project are honoured, like `.gitignore`.
- The global gitignore (`core.excludesFile`) applies.
- Files that are ignored but tracked anyway (`git add -f`) are no longer indexed.
- Exclude patterns are gitignore patterns: `.min.js` now means a file named `.min.js`, not every name ending in it; use `*.min.js`.
- Ignore files above the project folder are not read (as before), so a `~/.gitignore` doesn't affect projects under `$HOME`.

## [0.3.2] - 2026-10-10

### Fixed

- **Callers no longer disappear after a commit.** An incremental build (the one the post-commit hook runs) dropped every call into a changed file from files that had not changed, so `fn`, who-calls chains, impact analysis and dead-code results got worse with each commit until a `build --full`. Symbols in a changed file now keep their identity, and only that file's own calls and imports are re-resolved. Thanks @KennethWKZ.
- **Semantic search stays in sync with the code.** Edited, renamed and deleted symbols are re-embedded or removed on every build, not only in watch mode, and symbols whose embedding failed are retried on the next build. A failed request no longer drops a whole batch: it is split and retried, so one bad input loses only itself, and an unreachable provider stops after a few attempts instead of timing out hundreds of times. Thanks @KennethWKZ.
- **Local model servers (Ollama, LM Studio) no longer time out mid-build.** The request timeout is 300 s when `openai_base_url` points at this machine (30 s otherwise). Both the timeout and the batch size can be set in the `embedding` section of `config.json` (`request_timeout_secs`, `batch_size`).
- **Search results are stable.** With embeddings on, `graphmind search` and `gm_search` returned a different order from run to run and could list the same symbol twice. Results are now deterministic, and a symbol found by both keyword and semantic search appears once, with its real line and signature. Thanks @KennethWKZ.
- **Searches with punctuation work.** Queries such as `user-service`, `App\Models` or `getBalance()` used to return nothing; they are now matched literally.
- **`--offset` pages correctly.** Page 2 now holds the next results, not unrelated neighbours of page 1.

### Upgrading

Nothing to rebuild. Existing embedding indexes are kept; the next build only embeds what changed or failed before. To restore callers lost by earlier incremental builds, run `graphmind build --full` once.

`graphmind embed --run` now exits 1 when some symbols could not be embedded. `graphmind build` still exits 0.

## [0.3.1] - 2026-10-10

### Fixed

- **Updates via Homebrew (or any binary replacement) now finish configuring graphmind on their own.** After an update, graphmind re-runs its setup automatically, but that run was cut short when the command that started it ended, so it rarely completed — and a recent run by the previous version could postpone it by a day. It now runs as a separate background process, right after the update. In particular, the git hooks introduced in 0.3.0 (rebuild after merge, rebase and branch switch; registration of new worktrees) are now added to existing repos without running `graphmind setup` by hand. Updating with `graphmind update` was not affected.

## [0.3.0] - 2026-10-10

### Git worktree support

graphmind now works in git worktrees. Each checkout of a repo has its own code graph, so answers describe the branch you are on. Memory (decisions, conventions, bugs) is shared by all checkouts of the repo.

- **New worktrees work right away.** `git worktree add`, `claude --worktree`, or any graphmind command or MCP call made from a new checkout registers the worktree automatically. Repo identity is established by git, never guessed from folder names.
- **Claude Code's worktrees resolve to themselves.** Checkouts inside the main one (`.claude/worktrees/<name>`) used to be answered from the main checkout's graph, i.e. another branch. They now get their own.
- **Fast first build.** A new worktree's graph starts as a copy of the main checkout's, then updates in the background. Only the files that differ are parsed.
- **Shared memory.** A worktree reads and writes its main project's memory. `graphmind list` shows `[branch, worktree of <project>]`.
- **Agents switching worktrees mid-session.** MCP tools accept `project=<absolute path of the checkout>`, and the server instructions tell agents when to pass it.
- **Automatic cleanup.** Worktrees registered automatically are forgotten once their directory is deleted. `graphmind clean --stale` prunes the others.

### Git hooks

- New hooks rebuild the graph after **merge/pull**, **rebase/amend** and **branch switch**, in addition to commit. Thanks @KennethWKZ.
- Hooks are installed only in the repo's own git dir (shared by its worktrees). With husky or a shared `core.hooksPath`, graphmind changes nothing and prints the lines to add.
- Builds of the same project now run one at a time, so background builds started by hooks can't collide.

### Safer project resolution

- From a git repo that graphmind doesn't know, commands now fail with a clear error instead of answering from another registered project. Outside any git repo, a single registered project is still used as before (e.g. Claude Desktop).
- MCP calls that can't resolve a project list the registered projects in the error.
- Two checkouts with the same folder name no longer overwrite each other's registration.
- A damaged `config.json` is never replaced by an empty one.
- Writes to `config.json` and to memory files are locked across processes, so concurrent agents don't lose entries.

### Upgrading

No migration, nothing to rebuild. Existing projects, graphs and memory files are used as they are; projects registered before this version recognize their worktrees on first use. After the update, `graphmind setup` runs once on its own and adds the new git hooks to repos that already have graphmind's hooks — repos without them are left alone.

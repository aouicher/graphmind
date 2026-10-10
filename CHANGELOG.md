# Changelog

User-facing changes per release. Each entry becomes the release notes on GitHub.
Releases before 0.3.0: see [GitHub releases](https://github.com/aouicher/graphmind/releases).

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

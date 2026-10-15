# Changelog

User-facing changes per release. Each entry becomes the release notes on GitHub.
Releases before 0.3.0: see [GitHub releases](https://github.com/aouicher/graphmind/releases).

## [Unreleased]

### Fixed

- When it finds no graphmind, the desktop app installs one to `~/.local/bin`, like the install script, instead of `~/.graphmind/bin`; one it finds (Homebrew's included) is left alone, and a failed download no longer leaves `graphmind.tmp` behind. Thanks @KennethWKZ.
- Setup and `graphmind uninstall all` also remove the `~/.graphmind/bin` PATH line that desktop app versions 0.2.56 to 0.2.87 could add to `.bash_profile`. Thanks @KennethWKZ.

## [0.3.7] - 2026-10-15

### Fixed

- **Update checks call curl directly, not through rtk.** rtk 1.0 renamed `rtk proxy`, which graphmind used for its update checks; once rtk drops the old name, those checks would have failed. They never needed rtk. The CLI and the MCP server now share one daily check, so a failed check isn't retried by every MCP server start, and `graphmind update` reads the release tag from GitHub's JSON. Thanks @KennethWKZ for spotting the rename.

### Upgrading

Nothing to do.

## [0.3.6] - 2026-10-11

### Fixed

- **A removed worktree no longer lingers.** After `git worktree remove`, the next `graphmind list` or `graphmind build --all` (and the desktop app's project list and "build all") forgets the worktree and deletes its graph, so `build --all` no longer fails on its missing folder. Before, it stayed until another worktree of the repo was registered or `graphmind clean --stale` ran. Projects registered by hand are not affected.

### Upgrading

Nothing to do. Worktrees removed before this version are forgotten by the first `graphmind list` or `graphmind build --all`.

## [0.3.5] - 2026-10-11

### Fixed

- **`graphmind update` downloads to a file of its own.** The new binary is written next to the installed one, under a unique name created for this update, instead of a fixed name in the temp folder: two updates at once no longer corrupt each other, and on a shared machine no other user can put a file in its place. A downloaded binary that crashes now says why.
- **`graphmind build --all` says when a project fails.** It still builds every project, then exits non-zero and names the projects whose build failed, so a script or hook can tell. Thanks @KennethWKZ.
- **The install script no longer fails on a shell profile it can't read or write.** That profile is left as it is, and the script prints the PATH line to add yourself; before, it stopped with an error after installing the binary.

### Upgrading

Nothing to do. Scripts that run `graphmind build --all` now see a non-zero exit code when a project fails to build.

## [0.3.4] - 2026-10-11

### Fixed

- **A build never empties the graph when the project can't be read.** A project folder that exists but can't be read (permissions) fails the build and leaves the graph as it was, as a missing folder already did. A folder inside the project that can't be read keeps its files in the graph, with a warning, instead of having them removed.

### Upgrading

Nothing to do. If a build emptied a project's graph, run `graphmind build --full` on it once.

## [0.3.3] - 2026-10-11

### Added

- **`/gm` works outside an indexed project.** In a session opened elsewhere, the skill checks `graphmind which`, lists the registered projects and targets the one the question is about with `--slug` (asking only when several could match); a question about an unregistered folder goes to the agent's normal tools. `status`, `map`, `cycles`, `export` and `build` now take `--slug` like the other commands. Memory about a project is saved with `--slug <project>`; `--global` is for cross-project or personal facts.
- **Edit a memory in place.** `graphmind memory update <id>` and `gm_memory_update` change a fact's text, type, tags or priority without deleting and re-adding it (`--clear-tags` empties the tags). Thanks @KennethWKZ.
- **`graphmind uninstall skill`.** Turning skills off in the desktop app works; a skill folder that can't be removed is reported. Thanks @KennethWKZ.

### Fixed

- **graphmind is registered in Claude Code per project, pointing at the right binary.** One entry per repository in `~/.claude.json` (or `$CLAUDE_CONFIG_DIR/.claude.json`), under the path Claude Code reads from every worktree and subfolder. `graphmind init`, `graphmind register` and the desktop app write it; unregistering removes it. Setup repairs entries that point at a moved binary. A `graphmind` entry you set up by hand is left alone.
- **Setup never overwrites a file it can't read.** An unreadable or invalid `settings.json`, shell profile or git hook is left byte for byte as it is, with the reason printed. Files are backed up before they change, backups never replace one another, and every write goes through a temp file and a rename. MCP configs that don't parse and edited CLAUDE.md blocks are left as they are.
- **The setup summary says what happened.** "Setup complete" when everything was set up or left as you set it; "Setup finished — not everything was set up" when a file couldn't be read or written.
- **Indexing stays inside the project.** Files linked from outside the project, and links to files the project skips, are no longer indexed. Named pipes and devices no longer block the build. A build fails instead of emptying the graph when the project folder can't be read.
- **Ignored files stay out of the graph.** Exclude patterns and `.gitignore` files follow gitignore rules (`**`, root-anchored entries, `!` negation, nested `.gitignore` files, `.ignore`, the global gitignore). Before, many entries matched nothing, so ignored files were indexed and, with a remote embedding provider, sent to it. Thanks @KennethWKZ.
- **`gm_file` and `graphmind file` only serve indexed files of the project.** Thanks @KennethWKZ.
- **An unknown project is an error.** A slug or path that isn't a registered project fails with "Project '<x>' is not registered (see `graphmind list`)" in the CLI and in every MCP tool, instead of answering from another project. `graphmind unregister <typo>` no longer removes the current project. Memory tools only write for registered projects.
- **Claude Code hooks no longer lose their context.** Hook output stays under Claude Code's 10,000-character limit, priority memories are sent once per session instead of on every prompt, and two hooks that never ran are removed. Thanks @KennethWKZ.
- **The search nudge fires for the searches Claude Code actually runs**, including `rg`/`grep` through Bash and Explore agents.
- **Memory search ranks by what you searched for.** Common words are ignored, names in a fact count like tags, and results are stable from run to run. Thanks @KennethWKZ.
- **Memory no longer promotes almost everything to priority.** The prompt hook's searches don't count as recalls; the threshold is configurable (`memory.auto_promote_min_recalls`, 0 turns it off); entries an earlier version promoted are marked so they rank after the ones you saved as priority. Thanks @KennethWKZ.
- **Memory never loses entries** to concurrent writes, a failed write or an unreadable line. Thanks @KennethWKZ.
- **Semantic search embeds questions as questions.** Queries are sent to Voyage as queries, not documents; `embedding.query_prefix` adds an instruction for other models. Existing indexes keep working. Thanks @KennethWKZ.
- **Reading the graph never writes to it**, and the layout migration runs in one transaction, retried while the database is busy. Thanks @KennethWKZ.
- **PATH points at the real install folder, once,** and only when it's missing. Thanks @KennethWKZ.
- **`/gm` exists.** The main skill is named `gm`, as the instructions say; `/graphmind` keeps working. Thanks @KennethWKZ.
- **No notices or network checks in hooks and scripts,** and a failed update check is cached, so an offline machine doesn't wait on every command. An unreadable `config.json` is never replaced by defaults. Thanks @KennethWKZ.

### Changed

- Exclude patterns are gitignore patterns: `.min.js` now means a file named `.min.js`; use `*.min.js`. Files that are ignored but tracked anyway (`git add -f`) are no longer indexed.
- `graphmind search --code-only` and `gm_search` with `code_only` leave out markdown, yaml, toml and html symbols.

### Upgrading

Setup runs once on its own after the update: it rewrites graphmind's Claude Code entries per repository and updates the hooks, the skills and the CLAUDE.md block (an edited block is left as is). Nothing to rebuild; check your exclude patterns for the gitignore syntax above.

Going back to 0.3.2 after this update leaves the Claude Code hooks inactive (they call `graphmind which`, which 0.3.2 doesn't have). If you downgrade, run `graphmind setup` with 0.3.2.

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

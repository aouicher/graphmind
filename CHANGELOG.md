# Changelog

User-facing changes per release. Each entry becomes the release notes on GitHub.
Releases before 0.3.0: see [GitHub releases](https://github.com/aouicher/graphmind/releases).

## [Unreleased]

### Added

- **`/gm` works outside an indexed project.** In a session opened elsewhere, the skill checks `graphmind which`, runs `graphmind list`, picks the registered project the question is about (asking only when several could match) and passes `--slug <project>` to every command; a question about an unregistered folder goes to the agent's normal tools. `status`, `map`, `cycles`, `export` and `build` now take `--slug` like the other commands. The CLAUDE.md block says to use `/gm` for registered projects from outside them too, and to save memory about a project with `--slug <project>` (`--global` only for cross-project or personal facts). The skills' export instructions now match the CLI (whole-project export with `--format`; it has no `--symbol` or `--file`). Existing installs pick it up on the next auto-setup.

### Fixed

- Turning skills off in the desktop app works: `graphmind uninstall skill` exists. Thanks @KennethWKZ.
- **Setup never rewrites a settings.json or shell profile it can't read.** A `~/.claude/settings.json` that can't be read, isn't UTF-8 or doesn't parse is left byte-identical by `graphmind setup` and `graphmind install hook-claude` (which now exits 1), with the reason printed; the hook scripts of older versions are deleted only once their entries are gone from it. `graphmind uninstall` removes the hooks from settings.json first and deletes the scripts only when that worked. A shell profile (`~/.zshenv`, `~/.bashrc`, `~/.profile`, the PowerShell profile) that can't be read as text is left unchanged with "add <folder> to PATH yourself"; an existing profile is backed up before the PATH line is appended, and a missing one is created. Backups never replace one another, even two taken in the same second. The same holds for the git hooks setup adds next to graphmind's post-commit hook: one that can't be read as text, or can't be backed up, is left byte-identical with the reason. The setup summary says what each step did (PATH configured, configured in the profile but not in this shell yet, already on PATH, or left unchanged; hooks registered or not; skills installed or not). It reads "Setup complete" when everything was set up or deliberately left as you set it (an edited CLAUDE.md block, a hand-made MCP entry, each named on its own line), and "Setup finished — not everything was set up" when a file couldn't be read or written. `graphmind uninstall all` says when the hooks or the skills weren't removed.
- **An unknown project is an error.** A `--slug` (or positional slug, or MCP `project`) that is neither a registered slug nor the path of a registered checkout or worktree fails with "Project '<x>' is not registered (see `graphmind list`)", instead of answering from the current directory's project (`graphmind unregister <typo>` run inside a project removed that project). MCP responses add the registered projects, and every tool that takes `project` answers this way: `gm_search`, `gm_listeners`, `gm_cross_deps` and `gm_cross_links` (which now takes an optional `project`) used to return an empty result, and the memory tools use the same message. `.` and `..` name the current and parent directory, like `./src`.
- **Memory search ranks by what you searched for.** Applies to every memory search: `graphmind memory search`, `gm_memory_search` and the prompt hook. Common English and French words are ignored, so long facts no longer win on filler; repeating a word counts at most 3 times; terms of one or two letters match whole words only; with three or more search terms a fact must match at least two; and equal scores list the newest fact first, so results don't change from run to run. A search term written like a name (`fetchInBatches`, `FEATURE_GRADES`, `src/models/retry-policy.ts`, `web-app:direct`) that appears in a fact counts like one of its tags, a path matches its file name and a scoped name its parts, so facts no longer need the names repeated as `--tags`. Names written with `/`, `.`, `-` or `:`, or in camelCase, match this way only in direct `memory search` and `gm_memory_search` calls: the prompt hook keeps only `[a-z0-9_]`, so there only snake_case names do. A search made only of common words ("yes please", "why") finds nothing instead of listing the newest facts. Thanks @KennethWKZ.
- **Memory no longer promotes almost everything to priority.** Every memory search counted as a recall, including the one the prompt hook runs on every prompt, and consolidate made an entry priority after 3 recalls; on real installs most entries became "always injected" and pushed the facts saved as priority out of the capped list. The prompt hook's searches no longer count (auto-setup updates the hook), the threshold is configurable (`memory.auto_promote_min_recalls` in `config.json`, default 3, 0 turns auto-promotion off), and auto-promoted entries are marked `promoted` and listed after the ones saved as priority. Consolidate marks entries an earlier version auto-promoted (priority, 3 or more recalls, and changed after they were saved, which only auto-promotion did, before this version first ran); entries saved as priority, and entries pinned or edited since, are left as they are. It does this on every run, so a process still running the earlier version (an MCP server started before the upgrade) can't undo it by rewriting the file. Removing priority from an entry (`memory update --no-priority`) now sticks: consolidate doesn't promote it again. Thanks @KennethWKZ.
- **Ignored files stay out of the graph.** Exclude patterns and `.gitignore` files now follow gitignore rules (`**`, `*`, root-anchored entries like `/build`, `!` negation, nested `.gitignore` files). Before, wildcard and root-anchored entries silently matched nothing, so ignored files (generated code, credentials, private scripts) were indexed and, with a remote embedding provider, sent to it. Thanks @KennethWKZ.
- **No notices or network checks in pipes and hooks.** Runs whose stderr isn't a terminal (Claude Code hooks, git hooks, scripts) no longer check GitHub for updates and announcements or revalidate the license; they only run auto-setup (daily memory upkeep runs from the SessionStart hook). `graphmind mcp` revalidates the license itself, so MCP-only users keep their license. Thanks @KennethWKZ.
- **No repeated timeouts offline.** A failed update or announcement check is cached like a successful one, so an offline machine waits for it once a day instead of on every command.
- **Claude Code keeps graphmind per project, pointing at the right binary.** Claude Code gets one graphmind entry per repository, in `~/.claude.json` (`$CLAUDE_CONFIG_DIR/.claude.json` when that is set) under the path Claude Code reads for the repository from every worktree and subfolder: the main checkout, or the bare repository or the submodule for those; a folder outside git has its own. Registering a project (`graphmind init`, `graphmind register`, or adding it in the desktop app) writes or confirms that entry, through `claude mcp add -s local` when the `claude` CLI is on PATH; a `.claude.json` that doesn't parse never blocks the registration. Unregistering a project removes the entry once no registered project uses it, also after the project's folder was deleted. A `graphmind` entry you set up by hand (its command isn't a graphmind binary) is left alone. Setup, also run automatically after an upgrade, rewrites entries that point at a moved binary or carry an `env` block and moves the ones earlier versions wrote under a worktree's or a subfolder's own path to the repository's entry, in one write with one backup, and leaves current entries byte for byte as they are. A backup never replaces an earlier one. `graphmind uninstall all` and the desktop app's Claude Code card remove graphmind's entries and nothing else.
- **Config files written atomically.** Setup writes `~/.claude.json`, `~/.claude/settings.json`, `CLAUDE.md`, editor MCP configs, shell profiles and the hook scripts through a temp file and a rename, so Claude Code never reads a half-written file. Symlinked dotfiles stay links and keep their permissions; graphmind's own `~/.graphmind/config.json` is written the same way.
- **Claude Code hooks no longer lose their context.** Claude Code replaces hook output above 10,000 characters with a short preview, so on larger memory stores most of the injected memory never reached the model. The session and prompt hooks now cut their output to 9,000 characters, most specific first, and the prompt hook no longer repeats the priority list on every prompt: it adds only priority entries the session hasn't seen. Hook state lives in `~/.graphmind/hook-state`. Thanks @KennethWKZ.
- **Dead hooks retired.** The Stop and PostToolUse hooks read fields Claude Code doesn't send, so they never did anything while costing a process per event; setup removes them. Daily memory upkeep runs from the SessionStart hook.
- **Hook changes reach existing installs.** Auto-setup also runs when the installed hook scripts differ from the binary's, or when settings.json doesn't register them as this version does (an event, matcher or timeout), including from inside a hook.
- **The search nudge fires for the searches Claude Code runs.** Claude Code's native macOS and Linux builds have no Grep or Glob tools and search through Bash, so the nudge toward `/gm` now also fires for `ugrep`, `bfs`, and `rg`, `grep` and the other search commands after `cd … &&`, `||` or `;`, and for Glob calls and Explore agents where those exist. A command that runs graphmind, or a search whose output is piped on, is left alone.
- **Indexing stays inside the project.** Files linked from outside the project are no longer indexed, nor links to files the project skips (dotfiles, gitignored or excluded files). Named pipes, sockets and devices are skipped instead of blocking the build.
- **A build fails when the project directory can't be read.** It used to report 0 files and empty the graph; the graph is now left as it was and `graphmind build` exits non-zero.
- **Memory tools only accept registered projects.** `project` must be a registered slug or the path of a registered checkout or worktree; anything else returns an error listing the registered projects. Memory files are only ever created in `~/.graphmind/memory`.
- **`graphmind file` only shows indexed files of the project.**

### Changed

- `.ignore` files in the project are honoured, like `.gitignore`.
- The global gitignore (`core.excludesFile`) applies.
- Files that are ignored but tracked anyway (`git add -f`) are no longer indexed.
- Exclude patterns are gitignore patterns: `.min.js` now means a file named `.min.js`, not every name ending in it; use `*.min.js`.
- Ignore files above the project folder are not read (as before), so a `~/.gitignore` doesn't affect projects under `$HOME`.
- `graphmind search --code-only` and `gm_search` with `code_only` leave out markdown, yaml, toml and html symbols, so hook code prefetch isn't filled with doc headings.

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

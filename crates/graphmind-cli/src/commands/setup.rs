use colored::Colorize;
use graphmind_config::claude_settings::{self, PathRepair};
use graphmind_config::claude_code::{self, ClaudeCodeSync};
use graphmind_config::mcp_clients::{self, graphmind_binary};
use std::fs;
use std::path::{Path, PathBuf};

#[doc(hidden)]
pub use graphmind_config::mcp_clients::{McpClient, Outcome};

mod claude_md;
#[doc(hidden)]
pub use claude_md::{install_claude_md_block, uninstall_claude_md_block};
pub(crate) use claude_md::GM_INSTRUCTIONS;

mod shell_path;
#[doc(hidden)]
pub use shell_path::{already_on_path, install_shell_path, install_shell_path_for, uninstall_shell_path};

#[doc(hidden)]
pub use graphmind_config::paths::home_dir;

/// Global one-time setup: hooks, MCP configs, skill, CLAUDE.md instruction
pub fn setup() {
    println!(
        "\n{}  graphmind setup\n",
        "⚡".bold()
    );

    print_step(1, 10, "Shell PATH configuration");
    install_shell_path();

    print_step(2, 10, "Claude Code hooks");
    super::claude_hook::install_hook();

    print_step(3, 10, "Claude Code skills");
    super::install_skill::install_skill();

    print_step(4, 10, "Claude Desktop MCP config");
    let desktop = install_mcp_client(&McpClient::ClaudeDesktop);

    print_step(5, 10, "Claude Code MCP entries (per project)");
    let claude_code = install_claude_code_entries();
    // Separate from the entries: a settings.json that doesn't parse must not block them, and the
    // old settings.json entry goes only once they are in place.
    repair_claude_settings(claude_code.is_complete());

    print_step(6, 10, "OpenCode MCP config");
    let opencode = install_mcp_client(&McpClient::OpenCode);

    print_step(7, 10, "Cursor global MCP config");
    let cursor = install_mcp_client(&McpClient::Cursor);

    print_step(8, 10, "VS Code MCP entries of registered projects");
    refresh_vscode_entries();

    print_step(9, 10, "Git hooks of registered projects");
    match super::hooks::refresh_installed() {
        0 => println!("    {} up to date", "✓".green()),
        n => println!("    {} added new hooks in {n} repo(s)", "✓".green()),
    }

    print_step(10, 10, "CLAUDE.md instruction");
    let claude_md = install_claude_md_block();

    // Stamp setup version so CLI/desktop can detect outdated config. Not while a Claude Code entry
    // couldn't be brought up to date: auto-setup must retry it.
    if claude_code.is_complete() {
        if let Ok(mut config) = graphmind_config::read_config() {
            config.setup_version = graphmind_config::SETUP_VERSION;
            graphmind_config::save_config(&config);
        }
    }

    println!("\n{}", "─".repeat(50).dimmed());
    println!("{} Setup complete — v{}\n", "✓".green().bold(), env!("CARGO_PKG_VERSION"));
    println!("  {} PATH configured in shell profiles", "✓".green());
    println!("  {} 3 hooks registered (PreToolUse, SessionStart, UserPromptSubmit)", "✓".green());
    println!("  {} /gm skill + 19 sub-skills installed", "✓".green());
    let claude_code_summary = claude_code_outcome(&claude_code);
    let mut results = vec![(McpClient::ClaudeDesktop.label(), &desktop)];
    if let Some(outcome) = &claude_code_summary {
        results.push(("Claude Code (per project)", outcome));
    }
    results.extend([(McpClient::Cursor.label(), &cursor), (McpClient::OpenCode.label(), &opencode)]);
    print_mcp_summary(&results);
    if claude_code_summary.is_none() {
        println!(
            "  {} Claude Code: no project registered: run {} in a project",
            "⊘".yellow(),
            "graphmind init".cyan()
        );
    }
    match claude_md {
        Outcome::Written => println!("  {} CLAUDE.md instruction block updated", "✓".green()),
        Outcome::Unchanged => println!("  {} CLAUDE.md instruction block already current", "✓".green()),
        Outcome::NotInstalled | Outcome::LeftAlone(_) => {
            println!("  {} CLAUDE.md instruction block left as is (see above)", "⊘".yellow())
        }
    }
    if !claude_code.is_complete() {
        println!(
            "  {} setup not marked complete: it runs again until the Claude Code MCP entries are in place",
            "⊘".yellow()
        );
    }
    println!();
    println!("  Next: run {} in each project to index.", "graphmind init".cyan().bold());
    println!("  Update later with {}.", "graphmind update".dimmed());
    println!();
}

/// Per-project init: register, git hooks, build
pub fn init(path: Option<&str>, skip_build: bool) {
    let project_path = path.unwrap_or(".");

    println!(
        "\n{}  graphmind init ({})\n",
        "⚡".bold(),
        project_path
    );

    print_step(1, 4, &format!("Register project ({})", project_path));
    super::register::add(project_path, None, &[]);

    print_step(2, 4, "MCP project configs (Claude Code, VS Code)");
    ensure_project_mcp_configs(project_path);

    print_step(3, 4, "Git hooks (commit, merge, rebase, checkout, push)");
    super::hooks::install(None);

    if !skip_build {
        print_step(4, 4, "Build code graph");
        super::build::build(None, false, false, false, false);
    } else {
        println!("  {} Build skipped (use {} to index later)", "[4/4]".cyan().bold(), "graphmind build".dimmed());
    }

    println!("\n{}", "─".repeat(50).dimmed());
    println!("{} Project ready.\n", "✓".green().bold());
    println!("  Try: {} or {} or {}", "graphmind search \"<query>\"".cyan(), "graphmind fn <name>".cyan(), "graphmind map".cyan());
    println!();
}

fn print_step(n: u8, total: u8, label: &str) {
    println!("  {} {}", format!("[{n}/{total}]").cyan().bold(), label);
}

/// Add or update graphmind's MCP entry for `client` and say what happened. Claude Code goes
/// through `claude mcp add` when the CLI is available.
#[doc(hidden)]
pub fn install_mcp_client(client: &McpClient) -> Outcome {
    let outcome = mcp_clients::upsert(client, &graphmind_binary(), &mut print_cli_fallback);
    print_install(client, &outcome);
    outcome
}

/// Remove graphmind's MCP entry for `client` and say what happened.
#[doc(hidden)]
pub fn uninstall_mcp_client(client: &McpClient) -> Outcome {
    let outcome = mcp_clients::remove(client, &mut print_cli_fallback);
    match &outcome {
        Outcome::Written => println!("    {} removed from {}", "✓".green(), mcp_clients::describe(client)),
        Outcome::LeftAlone(reason) => println!("    {} {reason}", "✗".red()),
        Outcome::Unchanged | Outcome::NotInstalled => {}
    }
    outcome
}

fn print_cli_fallback(reason: &str) {
    println!("    {} {reason}; writing the file directly", "!".yellow());
}

fn print_install(client: &McpClient, outcome: &Outcome) {
    let path = mcp_clients::describe(client);
    match outcome {
        Outcome::Unchanged => println!("    {} already configured ({path})", "✓".green()),
        Outcome::Written => println!("    {} configured at {path}", "✓".green()),
        Outcome::NotInstalled => println!("    {} {} not installed, skipped", "⊘".yellow(), client.label()),
        Outcome::LeftAlone(reason) => println!("    {} {reason}", "✗".red()),
    }
}

fn print_mcp_summary(results: &[(&str, &Outcome)]) {
    let names = |wanted: fn(&Outcome) -> bool| {
        results
            .iter()
            .filter(|(_, outcome)| wanted(outcome))
            .map(|(name, _)| *name)
            .collect::<Vec<_>>()
            .join(", ")
    };
    let configured = names(Outcome::is_in_place);
    let skipped = names(|o| *o == Outcome::NotInstalled);
    let left_alone = names(|o| matches!(o, Outcome::LeftAlone(_)));
    if !configured.is_empty() {
        println!("  {} MCP server configured for {configured}", "✓".green());
    }
    if !skipped.is_empty() {
        println!("  {} not installed, skipped: {skipped}", "⊘".yellow());
    }
    if !left_alone.is_empty() {
        println!("  {} MCP config left unchanged for {left_alone} (see above)", "✗".red());
    }
}

/// Setup's Claude Code step. Claude Code reads graphmind per project, from
/// `projects.<key>.mcpServers` in .claude.json, one entry per repository (see
/// `claude_code::claude_code_key`; registering a project adds the entry): stale entries of registered projects are rewritten,
/// and a user-scope entry that runs graphmind is replaced by entries for the registered projects
/// (see `claude_code::sync_claude_code_projects`).
#[doc(hidden)]
pub fn install_claude_code_entries() -> ClaudeCodeSync {
    let user = McpClient::ClaudeCodeUser;
    let replace_user_scope = mcp_clients::has_graphmind_entry(&user);
    let sync = claude_code::sync_claude_code_projects(&graphmind_binary(), replace_user_scope, &mut print_cli_fallback);
    for key in &sync.written {
        println!("    {} configured for {}", "✓".green(), key.display());
    }
    for project in &sync.moved {
        println!(
            "    {} removed the entry under {}: Claude Code reads its repository's entry",
            "✓".green(),
            project.display()
        );
    }
    for key in &sync.hand_made {
        println!("    {} {}: left alone: hand-made graphmind entry", "⊘".yellow(), key.display());
    }
    for reason in &sync.left_alone {
        println!("    {} {reason}", "✗".red());
    }
    match &sync.user_scope {
        Outcome::Written => println!(
            "    {} removed the user-scope entry from {} (graphmind is registered per project)",
            "✓".green(),
            user.config_path().display()
        ),
        Outcome::LeftAlone(reason) if sync.left_alone.is_empty() => println!("    {} {reason}", "✗".red()),
        Outcome::LeftAlone(reason) if !sync.left_alone.contains(reason) => println!("    {} {reason}", "⊘".yellow()),
        Outcome::LeftAlone(_) => {}
        Outcome::Unchanged | Outcome::NotInstalled => {}
    }
    if sync.keys == 0 {
        println!("    {} no project registered: run {} in a project", "⊘".yellow(), "graphmind init".cyan());
    } else if sync.written.is_empty() && sync.moved.is_empty() && sync.is_complete() && sync.user_scope == Outcome::Unchanged {
        let n = sync.current;
        println!("    {} {n} project entr{} up to date", "✓".green(), if n == 1 { "y" } else { "ies" });
    }
    sync
}

/// The Claude Code step as one outcome, for the summary. `None` when no project is registered:
/// Claude Code has no graphmind entry then.
fn claude_code_outcome(sync: &ClaudeCodeSync) -> Option<Outcome> {
    if !sync.is_complete() {
        Some(Outcome::LeftAlone(String::new()))
    } else if sync.keys == 0 {
        None
    } else if !sync.written.is_empty() || !sync.moved.is_empty() || sync.user_scope == Outcome::Written {
        Some(Outcome::Written)
    } else {
        Some(Outcome::Unchanged)
    }
}

/// Repair ~/.claude/settings.json (see `claude_settings::repair_claude_settings`) and say what
/// changed. Setup sets `remove_legacy_entry` only once the Claude Code entries are in place.
#[doc(hidden)]
pub fn repair_claude_settings(remove_legacy_entry: bool) {
    let repair = match claude_settings::repair_claude_settings(remove_legacy_entry) {
        Ok(repair) => repair,
        Err(reason) => {
            println!("    {} {reason}", "✗".red());
            return;
        }
    };
    if repair.legacy_entry_removed {
        println!("    {} removed the old MCP entry from ~/.claude/settings.json", "✓".green());
    }
    match repair.path {
        PathRepair::NotNeeded => {}
        PathRepair::KeyRemoved => {
            println!(
                "    {} removed legacy env.PATH — Claude Code now inherits your shell PATH",
                "✓".green()
            );
        }
        PathRepair::Truncated { remaining } => {
            // We removed our segment but left the key, because it holds entries we
            // cannot prove are safe to discard. Say so — a surviving env.PATH still
            // overrides the shell PATH for every Bash tool call.
            println!(
                "    {} removed graphmind from env.PATH, but kept your other entries",
                "✓".green()
            );
            println!("      {}", format!("env.PATH = {remaining}").dimmed());
            println!(
                "      {} this still overrides your shell PATH for Bash tool calls.",
                "⚠".yellow()
            );
            println!(
                "      {}",
                "If you did not add those entries on purpose, delete env.PATH from"
                    .yellow()
            );
            println!(
                "      {}",
                "~/.claude/settings.json to inherit your full shell PATH.".yellow()
            );
        }
    }
}

/// VS Code entries of registered projects: an existing `.vscode/mcp.json` entry that isn't current
/// (an `env.PATH` block, a moved binary) is rewritten. Projects without one are left alone.
#[doc(hidden)]
pub fn refresh_vscode_entries() {
    let bin = graphmind_binary();
    let mut updated = 0usize;
    for project in graphmind_config::Registry::list() {
        let client = McpClient::VsCode(PathBuf::from(&project.path));
        if !mcp_clients::has_entry(&client) {
            continue;
        }
        match mcp_clients::upsert(&client, &bin, &mut print_cli_fallback) {
            Outcome::Written => updated += 1,
            Outcome::LeftAlone(reason) => println!("    {} {reason}", "✗".red()),
            Outcome::Unchanged | Outcome::NotInstalled => {}
        }
    }
    match updated {
        0 => println!("    {} up to date", "✓".green()),
        n => println!("    {} updated the VS Code entry in {n} project(s)", "✓".green()),
    }
}

/// The project's MCP entries: Claude Code's in .claude.json (see [`install_claude_code_project`]),
/// and VS Code's `<project>/.vscode/mcp.json`. The VS Code entry is written only in projects that
/// already use VS Code (have a `.vscode/` folder): elsewhere it would leave an untracked folder,
/// or a committed file pointing at this machine's binary.
#[doc(hidden)]
pub fn ensure_project_mcp_configs(project_path: &str) {
    let project = canonical_project_path(project_path);
    install_claude_code_project(&project);
    let client = McpClient::VsCode(project);
    let outcome = mcp_clients::upsert(&client, &graphmind_binary(), &mut print_cli_fallback);
    if outcome != Outcome::NotInstalled {
        print_install(&client, &outcome);
    }
}

/// The Claude Code entry `project` uses: `projects.<key>.mcpServers.graphmind` in .claude.json,
/// through `claude mcp add -s local` when the CLI is on PATH. The key is the folder Claude Code
/// reads for the project's repository, usually its main checkout (see
/// `claude_code::claude_code_key`); when that isn't the project's own folder (a worktree, a
/// subfolder), one line says which key was used.
#[doc(hidden)]
pub fn install_claude_code_project(project: &Path) -> Outcome {
    let key = claude_code::claude_code_key(project);
    if key != canonical_project_path(&project.to_string_lossy()) {
        println!(
            "    {} Claude Code entry under {}: Claude Code reads it there from every worktree and subfolder",
            "→".cyan(),
            key.display()
        );
    }
    install_mcp_client(&McpClient::ClaudeCodeProject(key))
}

fn canonical_project_path(project_path: &str) -> PathBuf {
    Path::new(project_path)
        .canonicalize()
        .unwrap_or_else(|_| PathBuf::from(project_path))
}

/// Remove graphmind from Claude Code (see `claude_code::uninstall_claude_code`), then graphmind's
/// MCP entry and `env.PATH` segment in ~/.claude/settings.json.
#[doc(hidden)]
pub fn unregister_mcp_in_claude_code() {
    let path = McpClient::ClaudeCodeUser.config_path();
    let removal = claude_code::uninstall_claude_code(&mut print_cli_fallback);
    match removal.projects {
        Ok(0) => {}
        Ok(n) => println!(
            "    {} removed {n} project entr{} from {}",
            "✓".green(),
            if n == 1 { "y" } else { "ies" },
            path.display()
        ),
        Err(reason) => println!("    {} {reason}", "✗".red()),
    }
    match removal.user_scope {
        Outcome::Written => println!("    {} removed the user-scope entry from {}", "✓".green(), path.display()),
        Outcome::LeftAlone(reason) => println!("    {} {reason}", "✗".red()),
        Outcome::Unchanged | Outcome::NotInstalled => {}
    }
    repair_claude_settings(true);
}

/// The Claude Code entry of an unregistered project (see `claude_code::remove_claude_code_project`)
/// and say what happened. A `graphmind` entry set up by hand, or one another registered project
/// uses, stays.
#[doc(hidden)]
pub fn uninstall_claude_code_project(project: &Path) -> Outcome {
    let outcome = claude_code::remove_claude_code_project(project, &mut print_cli_fallback);
    match &outcome {
        Outcome::Written => println!(
            "    {} removed the Claude Code entry from {}",
            "✓".green(),
            McpClient::ClaudeCodeUser.config_path().display()
        ),
        Outcome::LeftAlone(reason) => println!("    {} {reason}", "⊘".yellow()),
        Outcome::Unchanged | Outcome::NotInstalled => {}
    }
    outcome
}

/// Reverse everything `setup()` and `init()` did, across every registered project.
/// Indexed graphs/memory/config under `~/.graphmind` are preserved unless `purge` is set.
pub fn uninstall_all(purge: bool, yes: bool) {
    use std::io::Write as _;

    println!(
        "\n{}  graphmind uninstall all\n",
        "⚡".bold()
    );

    print_step(1, 10, "Claude Code hooks");
    super::claude_hook::uninstall_hook();

    print_step(2, 10, "Claude Code skills");
    let skills_result = super::install_skill::uninstall_skill();

    print_step(3, 10, "Claude Desktop MCP config");
    uninstall_mcp_client(&McpClient::ClaudeDesktop);

    print_step(4, 10, "Claude Code MCP server");
    unregister_mcp_in_claude_code();

    print_step(5, 10, "OpenCode MCP config");
    uninstall_mcp_client(&McpClient::OpenCode);

    print_step(6, 10, "Cursor global MCP config");
    uninstall_mcp_client(&McpClient::Cursor);

    print_step(7, 10, "CLAUDE.md instruction");
    uninstall_claude_md_block();

    print_step(8, 10, "Shell PATH configuration");
    uninstall_shell_path();

    // Step 4 removed every Claude Code project entry: only VS Code's are per project here.
    print_step(9, 10, "Per-project git hooks + VS Code MCP configs");
    let projects = graphmind_config::Registry::list();
    for project in &projects {
        super::hooks::uninstall(Some(&project.slug));
        uninstall_mcp_client(&McpClient::VsCode(canonical_project_path(&project.path)));
    }

    let mut config = graphmind_config::load_config();
    config.setup_version = 0;
    graphmind_config::save_config(&config);

    println!("\n{}", "─".repeat(50).dimmed());
    match &skills_result {
        Ok(()) => println!("{} Integrations removed.\n", "✓".green().bold()),
        Err(e) => println!(
            "{} Integrations removed, except the Claude Code skills: {}\n",
            "✗".red().bold(),
            e
        ),
    }
    println!(
        "  Indexed graphs, memory, and config under {} are {}.",
        graphmind_config::paths::graphmind_dir().display(),
        "PRESERVED".green().bold()
    );

    if purge {
        if !yes {
            println!();
            println!(
                "{}",
                "  ⚠ This will PERMANENTLY DELETE all indexed graphs, memory, and decisions for every project."
                    .red()
                    .bold()
            );
            print!("  Type 'yes' to confirm: ");
            std::io::stdout().flush().ok();
            let mut input = String::new();
            std::io::stdin().read_line(&mut input).ok();
            if input.trim() != "yes" {
                println!("\n  Aborted. Data preserved.\n");
                return;
            }
        }
        fs::remove_dir_all(graphmind_config::paths::graphmind_dir()).ok();
        println!("  {} ~/.graphmind data deleted", "✓".green());
    } else {
        println!(
            "  Run {} to also delete that data.",
            "graphmind uninstall all --purge".cyan().bold()
        );
    }

    println!();
    println!(
        "  Note: the graphmind binary itself was not removed. Delete it manually: {}",
        graphmind_binary().dimmed()
    );
    println!(
        "  You may still need to manually remove the now-orphaned PATH export from any shell profile this tool didn't touch (e.g. fish, custom profiles)."
    );
    println!();
}

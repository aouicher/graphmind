use colored::Colorize;
use graphmind_config::claude_settings::{self, PathRepair};
use graphmind_config::claude_home::{self, ClaudeHome, RecordedConfigDir};
use graphmind_config::mcp_clients::{self, graphmind_binary};
use std::fs;
use std::path::{Path, PathBuf};

#[doc(hidden)]
pub use graphmind_config::mcp_clients::{McpClient, Outcome};

mod claude_md;
#[doc(hidden)]
pub use claude_md::{install_claude_md_block, uninstall_claude_md_block};

mod shell_path;
#[doc(hidden)]
pub use shell_path::{already_on_path, install_shell_path, install_shell_path_for, uninstall_shell_path, ShellPath};

#[doc(hidden)]
pub use graphmind_config::paths::home_dir;

/// Global one-time setup: hooks, MCP configs, skill, CLAUDE.md instruction. Claude Code's part
/// (hooks, skills, MCP entry, CLAUDE.md) goes to the folder this process's environment names
/// ([`ClaudeHome::current`]), which is marked set up on its own.
pub fn setup() {
    println!(
        "\n{}  graphmind setup\n",
        "⚡".bold()
    );
    let home = ClaudeHome::current();
    if let Some(dir) = home.config_dir() {
        println!("  Claude Code folder: {} (CLAUDE_CONFIG_DIR)\n", dir.display());
    } else if let Some(value) = home.ignored() {
        println!("  {} {}\n", "!".yellow(), super::notices::relative_claude_config_dir_warning(value));
    }

    print_step(1, 10, "Shell PATH configuration");
    let shell_path = install_shell_path();

    print_step(2, 10, "Claude Code hooks");
    let hooks = super::claude_hook::install_hook(&home);

    print_step(3, 10, "Claude Code skills");
    let skills = super::install_skill::install_skill(&home);

    print_step(4, 10, "Claude Desktop MCP config");
    let desktop = install_mcp_client(&McpClient::ClaudeDesktop);

    print_step(5, 10, "Claude Code MCP server (user scope)");
    sync_claude_config_dir();
    let claude_code_client = home.mcp_client();
    let claude_code = install_mcp_client(&claude_code_client);
    // Separate from the registration: a settings.json that doesn't parse must not block it, and
    // the old settings.json entry goes only once the user-scope one is in place.
    repair_claude_settings(claude_code.is_in_place());

    print_step(6, 10, "OpenCode MCP config");
    let opencode = install_mcp_client(&McpClient::OpenCode);

    print_step(7, 10, "Cursor global MCP config");
    let cursor = install_mcp_client(&McpClient::Cursor);

    print_step(8, 10, "Project MCP entries (Claude Code project scope, VS Code)");
    migrate_project_mcp_entries(claude_code.is_in_place());

    print_step(9, 10, "Git hooks of registered projects");
    match super::hooks::refresh_installed() {
        0 => println!("    {} up to date", "✓".green()),
        n => println!("    {} added new hooks in {n} repo(s)", "✓".green()),
    }

    print_step(10, 10, "CLAUDE.md instruction");
    let claude_md = install_claude_md_block(&home);

    // Stamp this folder's setup version so CLI/desktop can detect outdated config. Not while the
    // Claude Code entry is missing: auto-setup must retry it, and the project-scope migration
    // waits for it. Hooks that couldn't be registered don't hold it back:
    // `installed_hooks_outdated()` sees installed scripts settings.json doesn't register, so
    // auto-setup retries them whatever the stamp says.
    let marked = if claude_code.is_in_place() {
        home.record_setup_version(graphmind_config::SETUP_VERSION).map(|gone| {
            for dir in gone {
                super::notices::remove_auto_setup_stamp(&dir);
            }
        })
    } else {
        Err("it runs again until the Claude Code MCP entry is in place".to_string())
    };

    println!("\n{}", "─".repeat(50).dimmed());
    if marked.is_ok() {
        println!("{} Setup complete — v{}\n", "✓".green().bold(), env!("CARGO_PKG_VERSION"));
    } else {
        println!("{} Setup finished — v{}\n", "⊘".yellow().bold(), env!("CARGO_PKG_VERSION"));
    }
    print_shell_path_summary(&shell_path);
    print_hooks_and_skills_summary(&hooks, &skills);
    print_mcp_summary(&[
        (McpClient::ClaudeDesktop.label(), &desktop),
        (claude_code_client.label(), &claude_code),
        (McpClient::Cursor.label(), &cursor),
        (McpClient::OpenCode.label(), &opencode),
    ]);
    match claude_md {
        Outcome::Written => println!("  {} CLAUDE.md instruction block updated", "✓".green()),
        Outcome::Unchanged => println!("  {} CLAUDE.md instruction block already current", "✓".green()),
        Outcome::NotInstalled | Outcome::LeftAlone(_) => {
            println!("  {} CLAUDE.md instruction block left as is (see above)", "⊘".yellow())
        }
    }
    if let Err(reason) = &marked {
        println!("  {} setup not marked complete: {reason}", "⊘".yellow());
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
    super::register::register(project_path, None, &[]);

    print_step(2, 4, "VS Code MCP config (projects with a .vscode/ folder)");
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
/// through `claude mcp add -s user` when the CLI is available.
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
        Outcome::Written => println!("    {} removed from {}", "✓".green(), client.config_path().display()),
        Outcome::LeftAlone(reason) => println!("    {} {reason}", "✗".red()),
        Outcome::Unchanged | Outcome::NotInstalled => {}
    }
    outcome
}

fn print_cli_fallback(reason: &str) {
    println!("    {} {reason}; writing the file directly", "!".yellow());
}

fn print_install(client: &McpClient, outcome: &Outcome) {
    let path = client.config_path();
    match outcome {
        Outcome::Unchanged => println!("    {} already configured ({})", "✓".green(), path.display()),
        Outcome::Written => println!("    {} configured at {}", "✓".green(), path.display()),
        Outcome::NotInstalled => println!("    {} {} not installed, skipped", "⊘".yellow(), client.label()),
        Outcome::LeftAlone(reason) => println!("    {} {reason}", "✗".red()),
    }
}

fn print_shell_path_summary(shell_path: &Result<ShellPath, String>) {
    match shell_path {
        Ok(ShellPath::Unchanged) => println!("  {} PATH already reaches graphmind", "✓".green()),
        Ok(ShellPath::AddedToProfile(profile)) => println!("  {} PATH configured in {}", "✓".green(), profile.display()),
        Ok(ShellPath::AddedToUserPath) => println!("  {} PATH configured in the user environment", "✓".green()),
        Err(reason) => println!("  {} PATH not configured: {reason}", "⊘".yellow()),
    }
}

fn print_hooks_and_skills_summary(hooks: &Result<(), String>, skills: &Result<(), String>) {
    let events = super::claude_hook::hook_events();
    if hooks.is_ok() {
        println!("  {} {} hooks registered ({})", "✓".green(), events.len(), events.join(", "));
    } else {
        println!("  {} Claude Code hooks not registered (see above)", "✗".red());
    }
    if skills.is_ok() {
        let count = super::install_skill::sub_skill_count();
        println!("  {} /gm skill + {count} sub-skills installed", "✓".green());
    } else {
        println!("  {} /gm skills not fully installed (see above)", "✗".red());
    }
}

/// Record `CLAUDE_CONFIG_DIR` for the desktop app, which doesn't inherit it when started from
/// Finder (see `claude_home::sync_claude_config_dir`). Say when the desktop app keeps using a
/// folder this setup doesn't set up.
fn sync_claude_config_dir() {
    match claude_home::sync_claude_config_dir() {
        Ok(RecordedConfigDir::Recorded(dir)) => println!(
            "    {} recorded CLAUDE_CONFIG_DIR ({}) for the desktop app",
            "✓".green(),
            dir.display()
        ),
        Ok(RecordedConfigDir::Cleared(dir)) => println!(
            "    {} dropped the recorded CLAUDE_CONFIG_DIR ({}): back to ~/.claude",
            "✓".green(),
            dir.display()
        ),
        Ok(RecordedConfigDir::Kept(dir)) => println!(
            "    {} this sets up ~/.claude; the desktop app keeps showing and setting up {} from an \
             earlier setup (run {} to drop it)",
            "!".yellow(),
            dir.display(),
            "CLAUDE_CONFIG_DIR= graphmind setup".cyan()
        ),
        Ok(RecordedConfigDir::Unchanged) => {}
        Err(reason) => println!("    {} {reason}", "✗".red()),
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

/// Repair ~/.claude/settings.json (see `claude_settings::repair_claude_settings`) and say what
/// changed. Setup sets `remove_legacy_entry` only once the user-scope entry is in place, so Claude
/// Code is never left without one.
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

/// Per-project MCP entries of registered projects.
///
/// - Claude Code: older versions wrote `projects.<path>.mcpServers.graphmind` into
///   ~/.claude.json on every `init`. A project entry takes precedence over the user-scope one,
///   so a stale `command` there would win. They are removed from every `.claude.json` graphmind
///   knows of (`claude_home::claude_code_files`), but only once the user-scope entry is in place
///   (`user_scope_in_place`).
/// - VS Code: an existing `.vscode/mcp.json` entry that isn't current (a pre-#105 `env.PATH`, a
///   moved binary) is rewritten. Projects without one are left alone.
#[doc(hidden)]
pub fn migrate_project_mcp_entries(user_scope_in_place: bool) {
    if user_scope_in_place {
        // Whether a line was printed for any file: a removal or an error.
        let mut reported = false;
        for target in claude_home::claude_code_files() {
            match mcp_clients::remove_claude_project_entries(&target, None) {
                Ok(0) => {}
                Ok(n) => {
                    reported = true;
                    println!(
                        "    {} removed {n} project-scope Claude Code entr{} from {} (the user-scope entry covers every project)",
                        "✓".green(),
                        if n == 1 { "y" } else { "ies" },
                        target.config_path().display()
                    );
                }
                Err(reason) => {
                    reported = true;
                    println!("    {} {reason}", "✗".red());
                }
            }
        }
        if !reported {
            println!("    {} no project-scope Claude Code entries", "✓".green());
        }
    } else {
        println!(
            "    {} project-scope Claude Code entries kept until the user-scope entry is in place",
            "⊘".yellow()
        );
    }

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
    if updated > 0 {
        println!("    {} updated the VS Code entry in {updated} project(s)", "✓".green());
    }
}

/// VS Code reads MCP servers per project, from `<project>/.vscode/mcp.json`. The entry is written
/// only in projects that already use VS Code (have a `.vscode/` folder): elsewhere it would leave
/// an untracked folder, or a committed file pointing at this machine's binary. Claude Code needs
/// nothing here: setup registers graphmind at user scope, which covers every project.
#[doc(hidden)]
pub fn ensure_project_mcp_configs(project_path: &str) {
    let client = McpClient::VsCode(canonical_project_path(project_path));
    let outcome = mcp_clients::upsert(&client, &graphmind_binary(), &mut print_cli_fallback);
    if outcome != Outcome::NotInstalled {
        print_install(&client, &outcome);
    }
}

fn canonical_project_path(project_path: &str) -> PathBuf {
    Path::new(project_path)
        .canonicalize()
        .unwrap_or_else(|_| PathBuf::from(project_path))
}

/// Remove the user-scope Claude Code entry from every `.claude.json` graphmind knows of
/// (`claude_home::claude_code_files`: this process's, ~/.claude.json, the recorded folder's and
/// each set-up folder's), plus what older versions left in ~/.claude/settings.json (the MCP entry
/// and the `env.PATH` segment).
#[doc(hidden)]
pub fn unregister_mcp_in_claude_code() {
    for target in claude_home::claude_code_files() {
        uninstall_mcp_client(&target);
    }
    // Reuse the #105 repair so uninstall and setup converge on identical results.
    repair_claude_settings(true);
}

/// Remove graphmind from one project's MCP configs: the project-scope entry older versions wrote
/// to `.claude.json` (every one `claude_home::claude_code_files` lists), and
/// `<project>/.vscode/mcp.json`. Parent entries and files stay.
#[doc(hidden)]
pub fn uninstall_project_mcp_configs(project_path: &str) {
    let abs_path = canonical_project_path(project_path);
    for target in claude_home::claude_code_files() {
        match mcp_clients::remove_claude_project_entries(&target, Some(&abs_path.to_string_lossy())) {
            Ok(0) => {}
            Ok(_) => println!("    {} removed from {} (project scope)", "✓".green(), target.config_path().display()),
            Err(reason) => println!("    {} {reason}", "✗".red()),
        }
    }
    uninstall_mcp_client(&McpClient::VsCode(abs_path));
}

/// Reverse everything `setup()` and `init()` did, across every registered project and every
/// Claude Code folder graphmind set up ([`ClaudeHome::all_known`]). Indexed graphs/memory/config
/// under `~/.graphmind` are preserved unless `purge` is set.
pub fn uninstall_all(purge: bool, yes: bool) {
    use std::io::Write as _;

    println!(
        "\n{}  graphmind uninstall all\n",
        "⚡".bold()
    );
    let homes = ClaudeHome::all_known();

    print_step(1, 10, "Claude Code hooks");
    for home in &homes {
        super::claude_hook::uninstall_hook(home);
    }

    print_step(2, 10, "Claude Code skills");
    for home in &homes {
        super::install_skill::uninstall_skill(home);
    }

    print_step(3, 10, "Claude Desktop MCP config");
    uninstall_mcp_client(&McpClient::ClaudeDesktop);

    print_step(4, 10, "Claude Code MCP server");
    unregister_mcp_in_claude_code();

    print_step(5, 10, "OpenCode MCP config");
    uninstall_mcp_client(&McpClient::OpenCode);

    print_step(6, 10, "Cursor global MCP config");
    uninstall_mcp_client(&McpClient::Cursor);

    print_step(7, 10, "CLAUDE.md instruction");
    for home in &homes {
        uninstall_claude_md_block(home);
    }

    print_step(8, 10, "Shell PATH configuration");
    uninstall_shell_path();

    print_step(9, 10, "Per-project git hooks + MCP configs");
    let projects = graphmind_config::Registry::list();
    for project in &projects {
        super::hooks::uninstall(Some(&project.slug));
        uninstall_project_mcp_configs(&project.path);
    }

    for home in &homes {
        match home.record_setup_version(0) {
            Ok(gone) => {
                for dir in gone.iter().map(PathBuf::as_path).chain(home.config_dir()) {
                    super::notices::remove_auto_setup_stamp(dir);
                }
            }
            Err(reason) => println!("    {} {reason}", "✗".red()),
        }
    }
    // The recorded CLAUDE_CONFIG_DIR goes too: graphmind was removed from that folder above, and
    // the desktop app would otherwise keep using it.
    let mut config = graphmind_config::load_config();
    config.claude_config_dir = None;
    graphmind_config::save_config(&config);

    println!("\n{}", "─".repeat(50).dimmed());
    println!("{} Integrations removed.\n", "✓".green().bold());
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

use colored::Colorize;
use serde_json::{json, Value};
use std::fs;
use std::path::PathBuf;

#[doc(hidden)]
pub fn home_dir() -> PathBuf {
    dirs::home_dir().expect("Cannot determine home directory")
}

/// Global one-time setup: hooks, MCP configs, skill, CLAUDE.md instruction
pub fn setup() {
    println!(
        "\n{}  graphmind setup\n",
        "⚡".bold()
    );

    print_step(1, 9, "Shell PATH configuration");
    install_shell_path();

    print_step(2, 9, "Claude Code hooks");
    super::claude_hook::install_hook();

    print_step(3, 9, "Claude Code skills");
    super::install_skill::install_skill();

    print_step(4, 9, "Claude Desktop MCP config");
    install_claude_desktop_mcp();

    print_step(5, 9, "Claude Code MCP server");
    register_mcp_in_claude_code();

    print_step(6, 9, "OpenCode MCP config");
    install_opencode_mcp();

    print_step(7, 9, "Cursor global MCP config");
    install_cursor_global_mcp();

    print_step(8, 9, "Repair legacy env.PATH (issue #105)");
    repair_project_mcp_env();

    print_step(9, 9, "CLAUDE.md instruction");
    install_claude_md_block();

    // Stamp setup version so CLI/desktop can detect outdated config
    let mut config = graphmind_config::load_config();
    config.setup_version = graphmind_config::SETUP_VERSION;
    graphmind_config::save_config(&config);

    println!("\n{}", "─".repeat(50).dimmed());
    println!("{} Setup complete — v{}\n", "✓".green().bold(), env!("CARGO_PKG_VERSION"));
    println!("  {} PATH configured in shell profiles", "✓".green());
    println!("  {} 5 hooks registered (PreToolUse, SessionStart, UserPromptSubmit, PostToolUse, Stop)", "✓".green());
    println!("  {} /gm skill + 19 sub-skills installed", "✓".green());
    println!("  {} MCP server configured (Claude Desktop + Claude Code + Cursor + OpenCode)", "✓".green());
    println!("  {} CLAUDE.md instruction block updated", "✓".green());
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

    print_step(2, 4, "MCP project configs (Claude Code, VS Code)");
    ensure_project_mcp_configs(project_path);

    print_step(3, 4, "Git hooks (post-commit + pre-push)");
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

#[doc(hidden)]
pub fn install_claude_desktop_mcp() {
    let config_path = claude_desktop_config_path();
    let Some(config_path) = config_path else {
        println!("    {} Claude Desktop config not found (not installed?)", "⊘".yellow());
        return;
    };

    let graphmind_path = find_graphmind_binary();

    let mut config: Value = if config_path.exists() {
        let content = fs::read_to_string(&config_path).unwrap_or_default();
        serde_json::from_str(&content).unwrap_or_else(|_| json!({}))
    } else {
        json!({})
    };

    let mcp_servers = config
        .as_object_mut()
        .unwrap()
        .entry("mcpServers")
        .or_insert_with(|| json!({}));

    // Backup before modifying
    if config_path.exists() {
        let ts = chrono::Utc::now().format("%Y%m%d_%H%M%S");
        let backup = config_path.with_extension(format!("json.{ts}.bak"));
        fs::copy(&config_path, &backup).ok();
    }

    if mcp_servers.get("graphmind").is_some() {
        println!("    {} already configured", "✓".green());
        return;
    }

    // No `env` block: `graphmind_path` is absolute, so the server needs nothing
    // from PATH to launch. Setting `env.PATH` here would REPLACE the inherited
    // environment and break nvm/asdf/mise/Homebrew-on-ARM setups. See issue #105.
    mcp_servers.as_object_mut().unwrap().insert(
        "graphmind".to_string(),
        json!({
            "command": graphmind_path,
            "args": ["mcp"]
        }),
    );

    if let Some(parent) = config_path.parent() {
        fs::create_dir_all(parent).ok();
    }
    let formatted = serde_json::to_string_pretty(&config).unwrap();
    fs::write(&config_path, formatted).unwrap_or_else(|e| {
        println!("    {} failed to write config: {e}", "✗".red());
    });
    println!("    {} configured at {}", "✓".green(), config_path.display());
}

#[doc(hidden)]
pub fn register_mcp_in_claude_code() {
    let settings_path = home_dir().join(".claude").join("settings.json");
    let graphmind_path = find_graphmind_binary();
    let graphmind_bin_dir = home_dir().join(".graphmind").join("bin").to_string_lossy().to_string();

    let mut config: Value = if settings_path.exists() {
        let content = fs::read_to_string(&settings_path).unwrap_or_default();
        serde_json::from_str(&content).unwrap_or_else(|_| json!({}))
    } else {
        json!({})
    };

    // Repair any env.PATH pollution written by graphmind <= v0.2.211.
    // We deliberately do NOT set a global env.PATH: it applies to every Bash tool
    // call in Claude Code, and the old code seeded it from a hardcoded
    // "/usr/local/bin:/usr/bin:/bin" default, wiping nvm/asdf/Homebrew-ARM paths.
    // The hook scripts already self-prefix PATH (see claude_hook.rs), so hooks
    // resolve `graphmind` without any settings.json help. See issue #105.
    repair_claude_settings_path(&mut config, &graphmind_bin_dir);

    let mcp_entry = json!({
        "command": graphmind_path,
        "args": ["mcp"]
    });

    let mcp_servers = config
        .as_object_mut()
        .unwrap()
        .entry("mcpServers")
        .or_insert_with(|| json!({}));

    // An entry is current only when it points at the right binary AND carries no
    // `env` block. Pre-#105 entries have `env.PATH` and must be rewritten.
    let already_correct = mcp_servers
        .get("graphmind")
        .is_some_and(|e| {
            e.get("env").is_none()
                && e.get("command").and_then(|c| c.as_str()) == Some(graphmind_path.as_str())
        });

    if !already_correct {
        mcp_servers.as_object_mut().unwrap().insert("graphmind".to_string(), mcp_entry);
    }

    // Backup before writing
    if settings_path.exists() {
        let ts = chrono::Utc::now().format("%Y%m%d_%H%M%S");
        let backup = settings_path.with_extension(format!("json.{ts}.bak"));
        fs::copy(&settings_path, &backup).ok();
    }

    let formatted = serde_json::to_string_pretty(&config).unwrap();
    fs::write(&settings_path, formatted).unwrap_or_else(|e| {
        println!("    {} failed to write settings: {e}", "✗".red());
    });
    println!("    {} configured", "✓".green());
}

/// Strip the stale `env.PATH` blocks that graphmind <= v0.2.211 wrote into every
/// per-project MCP entry of `~/.claude.json` and `<project>/.vscode/mcp.json`
/// (issue #105).
///
/// `setup()` only rewrites the *global* client configs; per-project entries would
/// otherwise keep their broken `env` until the user happened to re-run `init` in
/// each project. This sweeps them all in one pass.
///
/// Only the `env` key of graphmind's own entry is touched — other servers and any
/// user-added fields are left untouched.
#[doc(hidden)]
pub fn repair_project_mcp_env() {
    let mut repaired = 0usize;

    // 1. ~/.claude.json — every project-scoped graphmind entry
    let claude_json_path = home_dir().join(".claude.json");
    if claude_json_path.exists() {
        let content = fs::read_to_string(&claude_json_path).unwrap_or_default();
        if let Ok(mut config) = serde_json::from_str::<Value>(&content) {
            let mut changed = false;
            if let Some(projects) = config.get_mut("projects").and_then(|p| p.as_object_mut()) {
                for (_, project) in projects.iter_mut() {
                    if let Some(entry) = project
                        .get_mut("mcpServers")
                        .and_then(|m| m.get_mut("graphmind"))
                        .and_then(|e| e.as_object_mut())
                    {
                        if entry.remove("env").is_some() {
                            changed = true;
                            repaired += 1;
                        }
                    }
                }
            }
            if changed {
                let ts = chrono::Utc::now().format("%Y%m%d_%H%M%S");
                let backup = claude_json_path.with_extension(format!("json.{ts}.bak"));
                fs::copy(&claude_json_path, &backup).ok();
                let formatted = serde_json::to_string_pretty(&config).unwrap();
                fs::write(&claude_json_path, formatted).ok();
            }
        }
    }

    // 2. <project>/.vscode/mcp.json for every registered project
    for project in graphmind_config::Registry::list() {
        let vscode_mcp = std::path::Path::new(&project.path).join(".vscode").join("mcp.json");
        if !vscode_mcp.exists() {
            continue;
        }
        let content = fs::read_to_string(&vscode_mcp).unwrap_or_default();
        let Ok(mut config) = serde_json::from_str::<Value>(&content) else {
            continue;
        };
        let removed = config
            .get_mut("servers")
            .and_then(|s| s.get_mut("graphmind"))
            .and_then(|e| e.as_object_mut())
            .is_some_and(|e| e.remove("env").is_some());
        if removed {
            let ts = chrono::Utc::now().format("%Y%m%d_%H%M%S");
            let backup = vscode_mcp.with_extension(format!("json.{ts}.bak"));
            fs::copy(&vscode_mcp, &backup).ok();
            let formatted = serde_json::to_string_pretty(&config).unwrap();
            fs::write(&vscode_mcp, formatted).ok();
            repaired += 1;
        }
    }

    if repaired > 0 {
        println!(
            "    {} cleaned stale env.PATH from {repaired} project MCP entr{}",
            "✓".green(),
            if repaired == 1 { "y" } else { "ies" }
        );
    } else {
        println!("    {} no stale env.PATH found", "✓".green());
    }
}

/// Remove the `~/.graphmind/bin` segment that graphmind <= v0.2.211 injected into
/// the global `env.PATH` of `~/.claude/settings.json` (issue #105).
///
/// That key applies to every Bash tool call in Claude Code, and the old code
/// seeded it from a hardcoded `/usr/local/bin:/usr/bin:/bin` when absent — so
/// users on nvm/asdf/mise/Homebrew-ARM silently lost those directories.
///
/// The repair is deliberately conservative, because a user may have edited the
/// value by hand since:
///   - only the exact `~/.graphmind/bin` segment is dropped, wherever it sits;
///   - every other segment the user added is preserved, in order;
///   - if what remains is exactly the old hardcoded default, `env.PATH` is
///     removed entirely so Claude Code falls back to the inherited shell PATH
///     (the correct behaviour, and what nvm users need);
///   - anything else the user curated is left in place, minus our segment;
///   - an `env` object left empty is removed too, to avoid dead keys.
///
/// Returns true when the config was modified.
fn repair_claude_settings_path(config: &mut Value, graphmind_bin_dir: &str) -> bool {
    let Some(env) = config.get_mut("env").and_then(|e| e.as_object_mut()) else {
        return false;
    };
    let Some(current) = env.get("PATH").and_then(|p| p.as_str()) else {
        return false;
    };

    let kept: Vec<&str> = current
        .split(':')
        .filter(|seg| !seg.is_empty() && *seg != graphmind_bin_dir)
        .collect();

    if kept.len() == current.split(':').filter(|s| !s.is_empty()).count() {
        return false; // our segment was not there — nothing to repair
    }

    // If only the old hardcoded default remains, drop the key so the inherited
    // shell PATH wins. Anything else is user-curated and worth keeping.
    const OLD_DEFAULT: [&str; 3] = ["/usr/local/bin", "/usr/bin", "/bin"];
    if kept == OLD_DEFAULT {
        env.remove("PATH");
    } else {
        env.insert("PATH".to_string(), json!(kept.join(":")));
    }

    if env.is_empty() {
        config.as_object_mut().unwrap().remove("env");
    }
    true
}

#[doc(hidden)]
pub fn install_opencode_mcp() {
    let config_path = home_dir().join(".config").join("opencode").join("opencode.jsonc");
    let graphmind_path = find_graphmind_binary();

    let mut config: Value = if config_path.exists() {
        let content = fs::read_to_string(&config_path).unwrap_or_default();
        // Strip JSONC line comments before parsing
        let stripped: String = content.lines().map(|l| {
            if let Some(idx) = l.find("//") {
                // Only strip if // is not inside a string (simple heuristic)
                let before = &l[..idx];
                if before.chars().filter(|&c| c == '"').count() % 2 == 0 {
                    return before.trim_end().to_string();
                }
            }
            l.to_string()
        }).collect::<Vec<_>>().join("\n");
        serde_json::from_str(&stripped).unwrap_or_else(|_| json!({}))
    } else {
        json!({})
    };

    let mcp = config
        .as_object_mut()
        .unwrap()
        .entry("mcp")
        .or_insert_with(|| json!({}));

    // Backup before modifying
    if config_path.exists() {
        let ts = chrono::Utc::now().format("%Y%m%d_%H%M%S");
        let backup = config_path.with_extension(format!("jsonc.{ts}.bak"));
        fs::copy(&config_path, &backup).ok();
    }

    // Re-write when a stale pre-#105 entry carries an `environment` block.
    if mcp
        .get("graphmind")
        .is_some_and(|e| e.get("environment").is_none())
    {
        println!("    {} already configured", "✓".green());
        return;
    }

    // No `environment` block — the command array holds an absolute path. See issue #105.
    mcp.as_object_mut().unwrap().insert(
        "graphmind".to_string(),
        json!({
            "type": "local",
            "command": [graphmind_path, "mcp"]
        }),
    );

    if let Some(parent) = config_path.parent() {
        fs::create_dir_all(parent).ok();
    }
    let formatted = serde_json::to_string_pretty(&config).unwrap();
    fs::write(&config_path, formatted).unwrap_or_else(|e| {
        println!("    {} failed to write config: {e}", "✗".red());
    });

    if std::process::Command::new("which").arg("opencode").output().is_ok_and(|o| o.status.success()) {
        println!("    {} configured at {}", "✓".green(), config_path.display());
    } else {
        println!("    {} configured (opencode not detected, config written for future use)", "⊘".yellow());
    }
}

#[doc(hidden)]
pub fn install_cursor_global_mcp() {
    let config_path = home_dir().join(".cursor").join("mcp.json");
    let graphmind_path = find_graphmind_binary();

    let mut config: Value = if config_path.exists() {
        let content = fs::read_to_string(&config_path).unwrap_or_default();
        serde_json::from_str(&content).unwrap_or_else(|_| json!({}))
    } else {
        json!({})
    };

    // Re-write when a stale pre-#105 entry carries an `env` block.
    if config
        .get("mcpServers")
        .and_then(|m| m.get("graphmind"))
        .is_some_and(|e| e.get("env").is_none())
    {
        println!("    {} already configured", "✓".green());
        return;
    }

    // Backup before modifying
    if config_path.exists() {
        let ts = chrono::Utc::now().format("%Y%m%d_%H%M%S");
        let backup = config_path.with_extension(format!("json.{ts}.bak"));
        fs::copy(&config_path, &backup).ok();
    }

    let mcp_servers = config
        .as_object_mut()
        .unwrap()
        .entry("mcpServers")
        .or_insert_with(|| json!({}));
    // No `env` block — `graphmind_path` is absolute. See issue #105.
    mcp_servers.as_object_mut().unwrap().insert(
        "graphmind".to_string(),
        json!({
            "command": graphmind_path,
            "args": ["mcp"]
        }),
    );

    if let Some(parent) = config_path.parent() {
        fs::create_dir_all(parent).ok();
    }
    let formatted = serde_json::to_string_pretty(&config).unwrap();
    fs::write(&config_path, formatted).unwrap_or_else(|e| {
        println!("    {} failed to write config: {e}", "✗".red());
    });
    println!("    {} configured at {}", "✓".green(), config_path.display());
}

/// Ensure per-project MCP configs are in place for Claude Code (~/.claude.json)
/// and VS Code (<project>/.vscode/mcp.json).
/// Idempotent — safe to call on every build.
#[doc(hidden)]
pub fn ensure_project_mcp_configs(project_path: &str) {
    let abs_path = std::path::Path::new(project_path)
        .canonicalize()
        .unwrap_or_else(|_| std::path::PathBuf::from(project_path));
    let abs_str = abs_path.to_string_lossy().to_string();

    let graphmind_path = find_graphmind_binary();

    // 1. Claude Code — ~/.claude.json project-scoped entry
    {
        let claude_json_path = home_dir().join(".claude.json");
        let mut config: Value = if claude_json_path.exists() {
            let content = fs::read_to_string(&claude_json_path).unwrap_or_default();
            serde_json::from_str(&content).unwrap_or_else(|_| json!({}))
        } else {
            json!({})
        };

        // Stale pre-#105 entries carry `env.PATH` and must be rewritten.
        let already = config
            .get("projects")
            .and_then(|p| p.get(&abs_str))
            .and_then(|p| p.get("mcpServers"))
            .and_then(|m| m.get("graphmind"))
            .is_some_and(|e| e.get("env").is_none());

        if already {
            println!("    {} Claude Code (~/.claude.json) already configured", "✓".green());
        } else {
            if claude_json_path.exists() {
                let ts = chrono::Utc::now().format("%Y%m%d_%H%M%S");
                let backup = claude_json_path.with_extension(format!("json.{ts}.bak"));
                fs::copy(&claude_json_path, &backup).ok();
            }

            let projects = config
                .as_object_mut()
                .unwrap()
                .entry("projects")
                .or_insert_with(|| json!({}));
            let project_entry = projects
                .as_object_mut()
                .unwrap()
                .entry(abs_str.clone())
                .or_insert_with(|| json!({}));
            let mcp_servers = project_entry
                .as_object_mut()
                .unwrap()
                .entry("mcpServers")
                .or_insert_with(|| json!({}));
            // No `env` block — `graphmind_path` is absolute. See issue #105.
            mcp_servers.as_object_mut().unwrap().insert(
                "graphmind".to_string(),
                json!({
                    "type": "stdio",
                    "command": graphmind_path,
                    "args": ["mcp"]
                }),
            );

            let formatted = serde_json::to_string_pretty(&config).unwrap();
            fs::write(&claude_json_path, formatted).unwrap_or_else(|e| {
                println!("    {} failed to write ~/.claude.json: {e}", "✗".red());
            });
            println!("    {} Claude Code (~/.claude.json) configured", "✓".green());
        }
    }

    // 2. VS Code — <project>/.vscode/mcp.json
    {
        let vscode_dir = abs_path.join(".vscode");
        let vscode_mcp = vscode_dir.join("mcp.json");

        let mut config: Value = if vscode_mcp.exists() {
            let content = fs::read_to_string(&vscode_mcp).unwrap_or_default();
            serde_json::from_str(&content).unwrap_or_else(|_| json!({}))
        } else {
            json!({})
        };

        // Stale pre-#105 entries carry `env.PATH` and must be rewritten.
        let already = config
            .get("servers")
            .and_then(|m| m.get("graphmind"))
            .is_some_and(|e| e.get("env").is_none());

        if already {
            println!("    {} VS Code (.vscode/mcp.json) already configured", "✓".green());
        } else {
            if vscode_mcp.exists() {
                let ts = chrono::Utc::now().format("%Y%m%d_%H%M%S");
                let backup = vscode_mcp.with_extension(format!("json.{ts}.bak"));
                fs::copy(&vscode_mcp, &backup).ok();
            }

            let servers = config
                .as_object_mut()
                .unwrap()
                .entry("servers")
                .or_insert_with(|| json!({}));
            // No `env` block — `graphmind_path` is absolute. See issue #105.
            servers.as_object_mut().unwrap().insert(
                "graphmind".to_string(),
                json!({
                    "type": "stdio",
                    "command": graphmind_path,
                    "args": ["mcp"]
                }),
            );

            fs::create_dir_all(&vscode_dir).ok();
            let formatted = serde_json::to_string_pretty(&config).unwrap();
            fs::write(&vscode_mcp, formatted).unwrap_or_else(|e| {
                println!("    {} failed to write .vscode/mcp.json: {e}", "✗".red());
            });
            println!("    {} VS Code (.vscode/mcp.json) configured", "✓".green());
        }
    }
}

fn claude_desktop_config_path() -> Option<PathBuf> {
    let candidates: Vec<PathBuf> = if cfg!(target_os = "windows") {
        // %APPDATA%\Claude — derived from HOME so test HOME-override works
        vec![
            home_dir().join("AppData").join("Roaming").join("Claude").join("claude_desktop_config.json"),
        ]
    } else if cfg!(target_os = "macos") {
        vec![
            home_dir().join("Library/Application Support/Claude/claude_desktop_config.json"),
        ]
    } else {
        vec![
            home_dir().join(".config/claude/claude_desktop_config.json"),
        ]
    };
    for p in &candidates {
        if p.exists() {
            return Some(p.clone());
        }
    }
    candidates.into_iter().next()
}

fn find_graphmind_binary() -> String {
    // Prefer known install location — avoids relying on shell PATH
    let bin_name = if cfg!(target_os = "windows") { "graphmind.exe" } else { "graphmind" };
    let local_path = home_dir().join(".graphmind").join("bin").join(bin_name);
    if local_path.exists() {
        return local_path.to_string_lossy().to_string();
    }
    let which_cmd = if cfg!(target_os = "windows") { "where" } else { "which" };
    if let Ok(output) = std::process::Command::new(which_cmd)
        .arg("graphmind")
        .output()
    {
        if output.status.success() {
            let path = String::from_utf8_lossy(&output.stdout)
                .lines()
                .next()
                .unwrap_or("")
                .trim()
                .to_string();
            if !path.is_empty() {
                return path;
            }
        }
    }
    local_path.to_string_lossy().to_string()
}

fn gm_block() -> String {
    let version = env!("CARGO_PKG_VERSION");
    format!(r#"<!-- GM:START -->
<!-- GM:VERSION:{version} -->

## Mandatory: Code exploration via graphmind

In any graphmind-registered project, ALWAYS use `/gm` as the FIRST step for ANY code exploration, architecture understanding, or symbol lookup. This applies to:
- Answering questions about how something works
- Finding symbols, callers, dependencies, outlines
- Understanding code before making changes
- Exploring architecture or blast radius

**Do NOT** spawn Explore agents, use grep/find/Glob for code structure, or delegate code search to sub-agents until `/gm` has been tried first.

To **read** a symbol's source code, use `/gm-fn <symbol>` — it returns full source + callers + callees in one call. Prefer it over Read when you already know the symbol name.

Only fall back to grep for: string literals, config values, env vars, non-code patterns.

Invoke: `Skill(skill: "graphmind", args: "<your query>")`

**Auto-memory — mandatory behavior**: You MUST proactively save knowledge to graphmind memory **during** and after every session, without waiting for the user to ask. Save immediately when something is decided or discovered — do not wait for the end of the session.

Categories and commands:
- Architectural decisions → `graphmind memory add "<decision>" --type decision`
- Patterns/conventions → `--type pattern` or `--type convention`
- Bugs found or fixed → `--type bug`
- Critical always-needed facts → add `--priority` (injected every session)
- Cross-project or user-level facts → add `--global`

In a registered project: save without `--global` by default (project-scoped).
Outside a registered project: always use `--global`.

Be selective — only facts useful in a future session. Skip task details and temporary state.

<!-- GM:END -->"#)
}

#[doc(hidden)]
pub fn install_shell_path() {
    if cfg!(target_os = "windows") {
        install_shell_path_windows();
    } else {
        install_shell_path_unix();
    }
}

fn install_shell_path_unix() {
    let install_dir = home_dir().join(".graphmind").join("bin");
    let install_dir_str = install_dir.to_string_lossy();
    let export_line = format!("export PATH=\"{}:$PATH\"", install_dir_str);

    // Profiles to update — zshenv for non-interactive shells (MCP), zshrc/bashrc for interactive
    let profiles: &[&str] = &[".zshenv", ".zshrc", ".bashrc"];
    let mut updated = Vec::new();

    for profile in profiles {
        let path = home_dir().join(profile);
        // Only write to profiles that already exist, except .zshenv which we always ensure
        if !path.exists() && *profile != ".zshenv" {
            continue;
        }
        let content = if path.exists() {
            fs::read_to_string(&path).unwrap_or_default()
        } else {
            String::new()
        };
        if content.contains(&export_line) || content.contains(&format!("\"{}\"", install_dir_str)) {
            continue;
        }
        let new_content = if content.is_empty() {
            format!("{}\n", export_line)
        } else {
            format!("{}\n{}\n", content.trim_end(), export_line)
        };
        if let Err(e) = fs::write(&path, new_content) {
            println!("    {} failed to update {}: {e}", "✗".red(), profile);
        } else {
            updated.push(*profile);
        }
    }

    if updated.is_empty() {
        println!("    {} PATH already configured", "✓".green());
    } else {
        println!("    {} added to: {}", "✓".green(), updated.join(", "));
        println!("    {} restart your shell or run: source ~/{}", "→".cyan(), updated[0]);
    }
}

fn install_shell_path_windows() {
    let install_dir = home_dir().join(".graphmind").join("bin");
    let install_dir_str = install_dir.to_string_lossy().to_string();

    // 1. Add to user PATH via registry (persistent, non-interactive shells)
    let ps_set_path = format!(
        r#"$current = [Environment]::GetEnvironmentVariable('PATH', 'User'); if ($current -notlike '*{0}*') {{ [Environment]::SetEnvironmentVariable('PATH', '{0};' + $current, 'User'); Write-Output 'updated' }} else {{ Write-Output 'already' }}"#,
        install_dir_str
    );
    match std::process::Command::new("powershell")
        .args(["-NoProfile", "-Command", &ps_set_path])
        .output()
    {
        Ok(out) if out.status.success() => {
            let result = String::from_utf8_lossy(&out.stdout).trim().to_string();
            if result == "already" {
                println!("    {} PATH already configured (user environment)", "✓".green());
            } else {
                println!("    {} added to user PATH (registry)", "✓".green());
            }
        }
        Ok(out) => {
            let err = String::from_utf8_lossy(&out.stderr);
            println!("    {} failed to set user PATH: {}", "✗".red(), err.trim());
        }
        Err(e) => {
            println!("    {} powershell not available: {e}", "✗".red());
        }
    }

    // 2. Also add to PowerShell profile for interactive sessions
    let ps_get_profile = "$PROFILE";
    if let Ok(out) = std::process::Command::new("powershell")
        .args(["-NoProfile", "-Command", ps_get_profile])
        .output()
    {
        if out.status.success() {
            let profile_path = String::from_utf8_lossy(&out.stdout).trim().to_string();
            let profile_path = std::path::PathBuf::from(&profile_path);
            let add_line = format!("$env:PATH = '{};' + $env:PATH", install_dir_str);
            let content = if profile_path.exists() {
                fs::read_to_string(&profile_path).unwrap_or_default()
            } else {
                String::new()
            };
            if !content.contains(&install_dir_str) {
                if let Some(parent) = profile_path.parent() {
                    fs::create_dir_all(parent).ok();
                }
                let new_content = if content.is_empty() {
                    format!("{}\n", add_line)
                } else {
                    format!("{}\n{}\n", content.trim_end(), add_line)
                };
                if let Err(e) = fs::write(&profile_path, new_content) {
                    println!("    {} failed to update PowerShell profile: {e}", "✗".red());
                } else {
                    println!("    {} added to PowerShell profile", "✓".green());
                    println!("    {} restart PowerShell to pick up changes", "→".cyan());
                }
            }
        }
    }
}

#[doc(hidden)]
pub fn uninstall_shell_path() {
    if cfg!(target_os = "windows") {
        uninstall_shell_path_windows();
    } else {
        uninstall_shell_path_unix();
    }
}

fn uninstall_shell_path_unix() {
    let install_dir = home_dir().join(".graphmind").join("bin");
    let install_dir_str = install_dir.to_string_lossy().to_string();
    let export_line = format!("export PATH=\"{}:$PATH\"", install_dir_str);

    let profiles: &[&str] = &[".zshenv", ".zshrc", ".bashrc"];
    let mut updated = Vec::new();

    for profile in profiles {
        let path = home_dir().join(profile);
        if !path.exists() {
            continue;
        }
        let content = fs::read_to_string(&path).unwrap_or_default();
        if !content.contains(&export_line) {
            continue;
        }

        // Backup before modifying
        let ts = chrono::Utc::now().format("%Y%m%d_%H%M%S");
        let backup = path.with_extension(format!("{}.{ts}.bak", profile.trim_start_matches('.')));
        fs::copy(&path, &backup).ok();

        let lines: Vec<&str> = content.lines().collect();
        let mut new_lines: Vec<&str> = Vec::with_capacity(lines.len());
        let mut prev_blank = false;
        for line in lines {
            if line == export_line {
                // Drop the graphmind line; collapse an orphaned trailing blank line.
                continue;
            }
            if line.trim().is_empty() {
                if prev_blank {
                    continue;
                }
                prev_blank = true;
            } else {
                prev_blank = false;
            }
            new_lines.push(line);
        }
        let mut new_content = new_lines.join("\n");
        if !new_content.is_empty() {
            new_content.push('\n');
        }

        if let Err(e) = fs::write(&path, new_content) {
            println!("    {} failed to update {}: {e}", "✗".red(), profile);
        } else {
            updated.push(*profile);
        }
    }

    if updated.is_empty() {
        println!("    {} no graphmind PATH lines found", "✓".green());
    } else {
        println!("    {} removed from: {}", "✓".green(), updated.join(", "));
    }
}

fn uninstall_shell_path_windows() {
    let install_dir = home_dir().join(".graphmind").join("bin");
    let install_dir_str = install_dir.to_string_lossy().to_string();

    // 1. Remove from user PATH via registry
    let ps_remove_path = format!(
        r#"$current = [Environment]::GetEnvironmentVariable('PATH', 'User'); if ($current -like '*{0}*') {{ $parts = $current.Split(';') | Where-Object {{ $_ -ne '{0}' }}; [Environment]::SetEnvironmentVariable('PATH', ($parts -join ';'), 'User'); Write-Output 'removed' }} else {{ Write-Output 'absent' }}"#,
        install_dir_str
    );
    match std::process::Command::new("powershell")
        .args(["-NoProfile", "-Command", &ps_remove_path])
        .output()
    {
        Ok(out) if out.status.success() => {
            let result = String::from_utf8_lossy(&out.stdout).trim().to_string();
            if result == "absent" {
                println!("    {} PATH already clean (user environment)", "✓".green());
            } else {
                println!("    {} removed from user PATH (registry)", "✓".green());
            }
        }
        Ok(out) => {
            let err = String::from_utf8_lossy(&out.stderr);
            println!("    {} failed to update user PATH: {}", "✗".red(), err.trim());
        }
        Err(e) => {
            println!("    {} powershell not available: {e}", "✗".red());
        }
    }

    // 2. Remove the matching line from $PROFILE
    let ps_get_profile = "$PROFILE";
    if let Ok(out) = std::process::Command::new("powershell")
        .args(["-NoProfile", "-Command", ps_get_profile])
        .output()
    {
        if out.status.success() {
            let profile_path = String::from_utf8_lossy(&out.stdout).trim().to_string();
            let profile_path = std::path::PathBuf::from(&profile_path);
            let target_line = format!("$env:PATH = '{};' + $env:PATH", install_dir_str);
            if profile_path.exists() {
                let content = fs::read_to_string(&profile_path).unwrap_or_default();
                if content.contains(&target_line) {
                    let ts = chrono::Utc::now().format("%Y%m%d_%H%M%S");
                    let backup = profile_path.with_extension(format!("ps1.{ts}.bak"));
                    fs::copy(&profile_path, &backup).ok();

                    let new_content: String = content
                        .lines()
                        .filter(|l| *l != target_line)
                        .collect::<Vec<_>>()
                        .join("\n");
                    if let Err(e) = fs::write(&profile_path, format!("{new_content}\n")) {
                        println!("    {} failed to update PowerShell profile: {e}", "✗".red());
                    } else {
                        println!("    {} removed from PowerShell profile", "✓".green());
                    }
                }
            }
        }
    }
}

#[doc(hidden)]
pub fn uninstall_claude_desktop_mcp() {
    let Some(config_path) = claude_desktop_config_path() else {
        return;
    };
    if !config_path.exists() {
        return;
    }

    let content = fs::read_to_string(&config_path).unwrap_or_default();
    let mut config: Value = match serde_json::from_str(&content) {
        Ok(v) => v,
        Err(_) => return,
    };

    let had_entry = config
        .get("mcpServers")
        .and_then(|m| m.get("graphmind"))
        .is_some();
    if !had_entry {
        return;
    }

    let ts = chrono::Utc::now().format("%Y%m%d_%H%M%S");
    let backup = config_path.with_extension(format!("json.{ts}.bak"));
    fs::copy(&config_path, &backup).ok();

    if let Some(mcp_servers) = config.get_mut("mcpServers").and_then(|m| m.as_object_mut()) {
        mcp_servers.remove("graphmind");
    }

    let formatted = serde_json::to_string_pretty(&config).unwrap();
    fs::write(&config_path, formatted).unwrap_or_else(|e| {
        println!("    {} failed to write config: {e}", "✗".red());
    });
    println!("    {} removed from {}", "✓".green(), config_path.display());
}

#[doc(hidden)]
pub fn unregister_mcp_in_claude_code() {
    let settings_path = home_dir().join(".claude").join("settings.json");
    if !settings_path.exists() {
        return;
    }

    let content = fs::read_to_string(&settings_path).unwrap_or_default();
    let mut config: Value = match serde_json::from_str(&content) {
        Ok(v) => v,
        Err(_) => return,
    };

    let graphmind_bin_dir = home_dir().join(".graphmind").join("bin").to_string_lossy().to_string();

    let had_mcp_entry = config
        .get("mcpServers")
        .and_then(|m| m.get("graphmind"))
        .is_some();
    // Segment-aware: catches our directory wherever it sits, even if the user
    // reordered the value by hand (a plain strip_prefix would miss that).
    let has_path_segment = config
        .get("env")
        .and_then(|e| e.get("PATH"))
        .and_then(|p| p.as_str())
        .is_some_and(|p| p.split(':').any(|seg| seg == graphmind_bin_dir));

    if !had_mcp_entry && !has_path_segment {
        return;
    }

    let ts = chrono::Utc::now().format("%Y%m%d_%H%M%S");
    let backup = settings_path.with_extension(format!("json.{ts}.bak"));
    fs::copy(&settings_path, &backup).ok();

    if had_mcp_entry {
        if let Some(mcp_servers) = config.get_mut("mcpServers").and_then(|m| m.as_object_mut()) {
            mcp_servers.remove("graphmind");
        }
        // Drop an emptied mcpServers object rather than leaving a dead key.
        if config
            .get("mcpServers")
            .and_then(|m| m.as_object())
            .is_some_and(|m| m.is_empty())
        {
            config.as_object_mut().unwrap().remove("mcpServers");
        }
    }

    // Reuse the #105 repair so uninstall and setup converge on identical results.
    repair_claude_settings_path(&mut config, &graphmind_bin_dir);

    let formatted = serde_json::to_string_pretty(&config).unwrap();
    fs::write(&settings_path, formatted).unwrap_or_else(|e| {
        println!("    {} failed to write settings: {e}", "✗".red());
    });
    println!("    {} unregistered from Claude Code settings", "✓".green());
}

#[doc(hidden)]
pub fn uninstall_opencode_mcp() {
    let config_path = home_dir().join(".config").join("opencode").join("opencode.jsonc");
    if !config_path.exists() {
        return;
    }

    let content = fs::read_to_string(&config_path).unwrap_or_default();
    // Strip JSONC line comments before parsing (same heuristic as install_opencode_mcp)
    let stripped: String = content.lines().map(|l| {
        if let Some(idx) = l.find("//") {
            let before = &l[..idx];
            if before.chars().filter(|&c| c == '"').count() % 2 == 0 {
                return before.trim_end().to_string();
            }
        }
        l.to_string()
    }).collect::<Vec<_>>().join("\n");

    let mut config: Value = match serde_json::from_str(&stripped) {
        Ok(v) => v,
        Err(_) => return,
    };

    let had_entry = config
        .get("mcp")
        .and_then(|m| m.get("graphmind"))
        .is_some();
    if !had_entry {
        return;
    }

    let ts = chrono::Utc::now().format("%Y%m%d_%H%M%S");
    let backup = config_path.with_extension(format!("jsonc.{ts}.bak"));
    fs::copy(&config_path, &backup).ok();

    if let Some(mcp) = config.get_mut("mcp").and_then(|m| m.as_object_mut()) {
        mcp.remove("graphmind");
    }

    let formatted = serde_json::to_string_pretty(&config).unwrap();
    fs::write(&config_path, formatted).unwrap_or_else(|e| {
        println!("    {} failed to write config: {e}", "✗".red());
    });
    println!("    {} removed from {}", "✓".green(), config_path.display());
}

#[doc(hidden)]
pub fn uninstall_cursor_global_mcp() {
    let config_path = home_dir().join(".cursor").join("mcp.json");
    if !config_path.exists() {
        return;
    }

    let content = fs::read_to_string(&config_path).unwrap_or_default();
    let mut config: Value = match serde_json::from_str(&content) {
        Ok(v) => v,
        Err(_) => return,
    };

    let had_entry = config
        .get("mcpServers")
        .and_then(|m| m.get("graphmind"))
        .is_some();
    if !had_entry {
        return;
    }

    let ts = chrono::Utc::now().format("%Y%m%d_%H%M%S");
    let backup = config_path.with_extension(format!("json.{ts}.bak"));
    fs::copy(&config_path, &backup).ok();

    if let Some(mcp_servers) = config.get_mut("mcpServers").and_then(|m| m.as_object_mut()) {
        mcp_servers.remove("graphmind");
    }

    let formatted = serde_json::to_string_pretty(&config).unwrap();
    fs::write(&config_path, formatted).unwrap_or_else(|e| {
        println!("    {} failed to write config: {e}", "✗".red());
    });
    println!("    {} removed from {}", "✓".green(), config_path.display());
}

/// Reverse `ensure_project_mcp_configs` for a single project — removes only the
/// graphmind entries from `~/.claude.json` and `<project>/.vscode/mcp.json`,
/// never deleting the parent entries/files themselves.
#[doc(hidden)]
pub fn uninstall_project_mcp_configs(project_path: &str) {
    let abs_path = std::path::Path::new(project_path)
        .canonicalize()
        .unwrap_or_else(|_| std::path::PathBuf::from(project_path));
    let abs_str = abs_path.to_string_lossy().to_string();

    // 1. Claude Code — ~/.claude.json project-scoped entry
    {
        let claude_json_path = home_dir().join(".claude.json");
        if claude_json_path.exists() {
            let content = fs::read_to_string(&claude_json_path).unwrap_or_default();
            if let Ok(mut config) = serde_json::from_str::<Value>(&content) {
                let has_entry = config
                    .get("projects")
                    .and_then(|p| p.get(&abs_str))
                    .and_then(|p| p.get("mcpServers"))
                    .and_then(|m| m.get("graphmind"))
                    .is_some();

                if has_entry {
                    let ts = chrono::Utc::now().format("%Y%m%d_%H%M%S");
                    let backup = claude_json_path.with_extension(format!("json.{ts}.bak"));
                    fs::copy(&claude_json_path, &backup).ok();

                    if let Some(mcp_servers) = config
                        .get_mut("projects")
                        .and_then(|p| p.get_mut(&abs_str))
                        .and_then(|p| p.get_mut("mcpServers"))
                        .and_then(|m| m.as_object_mut())
                    {
                        mcp_servers.remove("graphmind");
                    }

                    let formatted = serde_json::to_string_pretty(&config).unwrap();
                    fs::write(&claude_json_path, formatted).unwrap_or_else(|e| {
                        println!("    {} failed to write ~/.claude.json: {e}", "✗".red());
                    });
                    println!("    {} removed from ~/.claude.json", "✓".green());
                }
            }
        }
    }

    // 2. VS Code — <project>/.vscode/mcp.json
    {
        let vscode_mcp = abs_path.join(".vscode").join("mcp.json");
        if vscode_mcp.exists() {
            let content = fs::read_to_string(&vscode_mcp).unwrap_or_default();
            if let Ok(mut config) = serde_json::from_str::<Value>(&content) {
                let has_entry = config
                    .get("servers")
                    .and_then(|m| m.get("graphmind"))
                    .is_some();

                if has_entry {
                    let ts = chrono::Utc::now().format("%Y%m%d_%H%M%S");
                    let backup = vscode_mcp.with_extension(format!("json.{ts}.bak"));
                    fs::copy(&vscode_mcp, &backup).ok();

                    if let Some(servers) = config.get_mut("servers").and_then(|m| m.as_object_mut()) {
                        servers.remove("graphmind");
                    }

                    let formatted = serde_json::to_string_pretty(&config).unwrap();
                    fs::write(&vscode_mcp, formatted).unwrap_or_else(|e| {
                        println!("    {} failed to write .vscode/mcp.json: {e}", "✗".red());
                    });
                    println!("    {} removed from .vscode/mcp.json", "✓".green());
                }
            }
        }
    }
}

#[doc(hidden)]
pub fn uninstall_claude_md_block() {
    let claude_md = home_dir().join(".claude").join("CLAUDE.md");
    if !claude_md.exists() {
        return;
    }

    let content = fs::read_to_string(&claude_md).unwrap_or_default();
    let Some(start) = content.find("<!-- GM:START -->") else {
        return;
    };

    // Backup before modifying (timestamped)
    let ts = chrono::Utc::now().format("%Y%m%d_%H%M%S");
    let backup = claude_md.with_extension(format!("md.{ts}.bak"));
    fs::write(&backup, &content).ok();

    let end = content.find("<!-- GM:END -->")
        .map(|i| i + "<!-- GM:END -->".len())
        .unwrap_or(content.len());

    let mut new_content = format!("{}{}", &content[..start], &content[end..]);

    // Collapse any run of 3+ consecutive newlines into exactly 2.
    while new_content.contains("\n\n\n") {
        new_content = new_content.replace("\n\n\n", "\n\n");
    }

    fs::write(&claude_md, new_content).unwrap_or_else(|e| {
        println!("    {} failed to write CLAUDE.md: {e}", "✗".red());
    });
    println!("    {} instruction block removed", "✓".green());
}

/// Reverse everything `setup()` and `init()` did, across every registered project.
/// Indexed graphs/memory/config under `~/.graphmind` are preserved unless `purge` is set.
pub fn uninstall_all(purge: bool, yes: bool) {
    use std::io::Write as _;

    println!(
        "\n{}  graphmind uninstall all\n",
        "⚡".bold()
    );

    print_step(1, 9, "Claude Code hooks");
    super::claude_hook::uninstall_hook();

    print_step(2, 9, "Claude Code skills");
    super::install_skill::uninstall_skill();

    print_step(3, 9, "Claude Desktop MCP config");
    uninstall_claude_desktop_mcp();

    print_step(4, 9, "Claude Code MCP server");
    unregister_mcp_in_claude_code();

    print_step(5, 9, "OpenCode MCP config");
    uninstall_opencode_mcp();

    print_step(6, 9, "Cursor global MCP config");
    uninstall_cursor_global_mcp();

    print_step(7, 9, "CLAUDE.md instruction");
    uninstall_claude_md_block();

    print_step(8, 9, "Shell PATH configuration");
    uninstall_shell_path();

    print_step(9, 9, "Per-project git hooks + MCP configs");
    let projects = graphmind_config::Registry::list();
    for project in &projects {
        super::hooks::uninstall(Some(&project.slug));
        uninstall_project_mcp_configs(&project.path);
    }

    let mut config = graphmind_config::load_config();
    config.setup_version = 0;
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
        find_graphmind_binary().dimmed()
    );
    println!(
        "  You may still need to manually remove the now-orphaned PATH export from any shell profile this tool didn't touch (e.g. fish, custom profiles)."
    );
    println!();
}

#[doc(hidden)]
pub fn install_claude_md_block() {
    let claude_md = home_dir().join(".claude").join("CLAUDE.md");

    let content = if claude_md.exists() {
        let c = fs::read_to_string(&claude_md).unwrap_or_default();
        // Backup before modifying (timestamped)
        let ts = chrono::Utc::now().format("%Y%m%d_%H%M%S");
        let backup = claude_md.with_extension(format!("md.{ts}.bak"));
        fs::write(&backup, &c).ok();
        c
    } else {
        String::new()
    };

    let new_content = if let Some(start) = content.find("<!-- GM:START -->") {
        // Replace existing block in place — use char-safe slicing via find (returns byte indices on valid UTF-8 boundaries)
        let end = content.find("<!-- GM:END -->")
            .map(|i| i + "<!-- GM:END -->".len())
            .unwrap_or(content.len());
        format!("{}{}{}", &content[..start], gm_block(), &content[end..])
    } else if let Some(omc_start) = content.find("<!-- OMC:START -->") {
        // Insert before OMC block — highest attention weight position
        format!("{}{}\n\n{}", &content[..omc_start], gm_block(), &content[omc_start..])
    } else {
        // No existing block — prepend before other content
        format!("{}\n\n{}", gm_block(), content)
    };

    fs::create_dir_all(claude_md.parent().unwrap()).ok();
    fs::write(&claude_md, new_content).unwrap_or_else(|e| {
        println!("    {} failed to write CLAUDE.md: {e}", "✗".red());
    });
    println!("    {} instruction block installed", "✓".green());
}

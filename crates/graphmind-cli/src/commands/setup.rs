use colored::Colorize;
use serde_json::{json, Value};
use std::fs;
use std::path::PathBuf;

fn home_dir() -> PathBuf {
    dirs::home_dir().expect("Cannot determine home directory")
}

/// Global one-time setup: hooks, MCP configs, skill, CLAUDE.md instruction
pub fn setup() {
    println!(
        "\n{}  graphmind setup\n",
        "⚡".bold()
    );

    print_step(1, 7, "Shell PATH configuration");
    install_shell_path();

    print_step(2, 7, "Claude Code hooks");
    super::claude_hook::install_hook();

    print_step(3, 7, "Claude Code skills");
    super::install_skill::install_skill();

    print_step(4, 7, "Claude Desktop MCP config");
    install_claude_desktop_mcp();

    print_step(5, 7, "Claude Code MCP server");
    register_mcp_in_claude_code();

    print_step(6, 7, "OpenCode MCP config");
    install_opencode_mcp();

    print_step(7, 7, "CLAUDE.md instruction");
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
    println!("  {} MCP server configured (Claude Desktop + Claude Code + OpenCode)", "✓".green());
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

    print_step(1, 3, &format!("Register project ({})", project_path));
    super::register::register(project_path, None, &[]);

    print_step(2, 3, "Git hooks (post-commit + pre-push)");
    super::hooks::install(None);

    if !skip_build {
        print_step(3, 3, "Build code graph");
        super::build::build(None, false, false, false, false);
    } else {
        println!("  {} Build skipped (use {} to index later)", "[3/3]".cyan().bold(), "graphmind build".dimmed());
    }

    println!("\n{}", "─".repeat(50).dimmed());
    println!("{} Project ready.\n", "✓".green().bold());
    println!("  Try: {} or {} or {}", "graphmind search \"<query>\"".cyan(), "graphmind fn <name>".cyan(), "graphmind map".cyan());
    println!();
}

fn print_step(n: u8, total: u8, label: &str) {
    println!("  {} {}", format!("[{n}/{total}]").cyan().bold(), label);
}

fn install_claude_desktop_mcp() {
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

    if mcp_servers.get("graphmind").is_some() {
        println!("    {} already configured", "✓".green());
        return;
    }

    let graphmind_bin_dir = home_dir().join(".graphmind").join("bin").to_string_lossy().to_string();
    mcp_servers.as_object_mut().unwrap().insert(
        "graphmind".to_string(),
        json!({
            "command": graphmind_path,
            "args": ["mcp"],
            "env": {
                "PATH": format!("{}:/usr/local/bin:/usr/bin:/bin", graphmind_bin_dir)
            }
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

fn register_mcp_in_claude_code() {
    let settings_path = home_dir().join(".claude").join("settings.json");
    let graphmind_path = find_graphmind_binary();
    let graphmind_bin_dir = home_dir().join(".graphmind").join("bin").to_string_lossy().to_string();

    let mut config: Value = if settings_path.exists() {
        let content = fs::read_to_string(&settings_path).unwrap_or_default();
        serde_json::from_str(&content).unwrap_or_else(|_| json!({}))
    } else {
        json!({})
    };

    // Inject ~/.graphmind/bin into the global env.PATH so hooks (Bash tool) can resolve graphmind
    {
        let env = config
            .as_object_mut()
            .unwrap()
            .entry("env")
            .or_insert_with(|| json!({}));
        let current_path = env
            .get("PATH")
            .and_then(|v| v.as_str())
            .unwrap_or("/usr/local/bin:/usr/bin:/bin")
            .to_string();
        if !current_path.contains(&graphmind_bin_dir) {
            let new_path = format!("{}:{}", graphmind_bin_dir, current_path);
            env.as_object_mut().unwrap().insert("PATH".to_string(), json!(new_path));
        }
    }

    let mcp_entry = json!({
        "command": graphmind_path,
        "args": ["mcp"],
        "env": {
            "PATH": format!("{}:/usr/local/bin:/usr/bin:/bin", graphmind_bin_dir)
        }
    });

    let mcp_servers = config
        .as_object_mut()
        .unwrap()
        .entry("mcpServers")
        .or_insert_with(|| json!({}));

    let already_correct = mcp_servers
        .get("graphmind")
        .and_then(|e| e.get("env"))
        .is_some();

    if !already_correct {
        mcp_servers.as_object_mut().unwrap().insert("graphmind".to_string(), mcp_entry);
    }

    let formatted = serde_json::to_string_pretty(&config).unwrap();
    fs::write(&settings_path, formatted).unwrap_or_else(|e| {
        println!("    {} failed to write settings: {e}", "✗".red());
    });
    println!("    {} configured", "✓".green());
}

fn install_opencode_mcp() {
    let config_path = home_dir().join(".config").join("opencode").join("opencode.jsonc");
    let graphmind_path = find_graphmind_binary();
    let graphmind_bin_dir = home_dir().join(".graphmind").join("bin").to_string_lossy().to_string();

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

    if mcp.get("graphmind").is_some() {
        println!("    {} already configured", "✓".green());
        return;
    }

    mcp.as_object_mut().unwrap().insert(
        "graphmind".to_string(),
        json!({
            "type": "local",
            "command": [graphmind_path, "mcp"],
            "environment": {
                "PATH": format!("{}:/usr/local/bin:/usr/bin:/bin", graphmind_bin_dir)
            }
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

fn claude_desktop_config_path() -> Option<PathBuf> {
    let candidates = [
        home_dir().join("Library/Application Support/Claude/claude_desktop_config.json"),
        home_dir().join(".config/claude/claude_desktop_config.json"),
    ];
    for p in &candidates {
        if p.exists() {
            return Some(p.clone());
        }
    }
    if cfg!(target_os = "macos") {
        Some(candidates[0].clone())
    } else {
        Some(candidates[1].clone())
    }
}

fn find_graphmind_binary() -> String {
    // Prefer known install location — avoids relying on shell PATH
    let local_path = home_dir().join(".graphmind").join("bin").join("graphmind");
    if local_path.exists() {
        return local_path.to_string_lossy().to_string();
    }
    if let Ok(output) = std::process::Command::new("which")
        .arg("graphmind")
        .output()
    {
        if output.status.success() {
            let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
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

fn install_shell_path() {
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

fn install_claude_md_block() {
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

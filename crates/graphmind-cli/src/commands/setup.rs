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

    print_step(1, 5, "Claude Code hooks");
    super::claude_hook::install_hook();

    print_step(2, 5, "Claude Code skills");
    super::install_skill::install_skill();

    print_step(3, 5, "Claude Desktop MCP config");
    install_claude_desktop_mcp();

    print_step(4, 5, "Claude Code MCP server");
    register_mcp_in_claude_code();

    print_step(5, 5, "CLAUDE.md instruction");
    install_claude_md_block();

    // Stamp setup version so CLI/desktop can detect outdated config
    let mut config = graphmind_config::load_config();
    config.setup_version = graphmind_config::SETUP_VERSION;
    graphmind_config::save_config(&config);

    println!("\n{}", "─".repeat(50).dimmed());
    println!("{} Setup complete — v{}\n", "✓".green().bold(), env!("CARGO_PKG_VERSION"));
    println!("  {} 4 hooks registered (PreToolUse, SessionStart, UserPromptSubmit, PostToolUse)", "✓".green());
    println!("  {} /gm skill + 18 sub-skills installed", "✓".green());
    println!("  {} MCP server configured (Claude Desktop + Claude Code)", "✓".green());
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
        super::build::build(None, false, false, false);
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

fn register_mcp_in_claude_code() {
    let settings_path = home_dir().join(".claude").join("settings.json");
    let graphmind_path = find_graphmind_binary();

    let mut config: Value = if settings_path.exists() {
        let content = fs::read_to_string(&settings_path).unwrap_or_default();
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

    mcp_servers.as_object_mut().unwrap().insert(
        "graphmind".to_string(),
        json!({
            "command": graphmind_path,
            "args": ["mcp"]
        }),
    );

    let formatted = serde_json::to_string_pretty(&config).unwrap();
    fs::write(&settings_path, formatted).unwrap_or_else(|e| {
        println!("    {} failed to write settings: {e}", "✗".red());
    });
    println!("    {} configured", "✓".green());
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
    "graphmind".to_string()
}

const GM_BLOCK: &str = r#"<!-- GM:START -->
<!-- GM:VERSION:0.2.82 -->

## Mandatory: Code exploration via graphmind

In any graphmind-registered project, ALWAYS use `/gm` as the FIRST step for ANY code exploration, architecture understanding, or symbol lookup. This applies to:
- Answering questions about how something works
- Finding symbols, callers, dependencies, outlines
- Understanding code before making changes
- Exploring architecture or blast radius

**Do NOT** spawn Explore agents, use grep/find/Glob for code structure, or delegate code search to sub-agents until `/gm` has been tried first.

Only fall back to grep for: string literals, config values, env vars, non-code patterns.

Invoke: `Skill(skill: "graphmind", args: "<your query>")`

<!-- GM:END -->"#;

fn install_claude_md_block() {
    let claude_md = home_dir().join(".claude").join("CLAUDE.md");

    let content = if claude_md.exists() {
        fs::read_to_string(&claude_md).unwrap_or_default()
    } else {
        String::new()
    };

    let new_content = if content.contains("<!-- GM:START -->") {
        // Replace existing block in place
        let re_start = content.find("<!-- GM:START -->").unwrap();
        let re_end = content.find("<!-- GM:END -->")
            .map(|i| i + "<!-- GM:END -->".len())
            .unwrap_or(content.len());
        format!("{}{}{}", &content[..re_start], GM_BLOCK, &content[re_end..])
    } else if content.contains("<!-- OMC:START -->") {
        // Insert before OMC block — highest attention weight position
        let omc_start = content.find("<!-- OMC:START -->").unwrap();
        format!("{}{}\n\n{}", &content[..omc_start], GM_BLOCK, &content[omc_start..])
    } else {
        // No OMC block — prepend before other content
        format!("{}\n\n{}", GM_BLOCK, content)
    };

    fs::create_dir_all(claude_md.parent().unwrap()).ok();
    fs::write(&claude_md, new_content).unwrap_or_else(|e| {
        println!("    {} failed to write CLAUDE.md: {e}", "✗".red());
    });
    println!("    {} instruction block installed", "✓".green());
}

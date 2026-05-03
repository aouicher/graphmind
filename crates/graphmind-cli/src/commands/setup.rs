use colored::Colorize;
use serde_json::{json, Value};
use std::fs;
use std::path::PathBuf;

fn home_dir() -> PathBuf {
    dirs::home_dir().expect("Cannot determine home directory")
}

/// Global one-time setup: hooks, MCP configs, skill
pub fn setup() {
    println!(
        "\n{}  graphmind setup\n",
        "⚡".bold()
    );

    print_step(1, 4, "Claude Code hooks");
    super::claude_hook::install_hook();

    print_step(2, 4, "Claude Code skill");
    super::install_skill::install_skill();

    print_step(3, 4, "Claude Desktop MCP config");
    install_claude_desktop_mcp();

    print_step(4, 4, "Claude Code MCP server");
    register_mcp_in_claude_code();

    // Stamp setup version so CLI/desktop can detect outdated config
    let mut config = graphmind_config::load_config();
    config.setup_version = graphmind_config::SETUP_VERSION;
    graphmind_config::save_config(&config);

    println!("\n{}", "─".repeat(50).dimmed());
    println!("{} Global setup complete.\n", "✓".green().bold());
    println!("  Now run {} in each project you want to index.", "graphmind init".cyan().bold());
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
        print_step(3, 3, "Build code graph (skipped)");
    }

    println!("\n{}", "─".repeat(50).dimmed());
    println!("{} Project ready.\n", "✓".green().bold());
    println!("  {} graphmind search \"<query>\"", "→".cyan());
    println!("  {} graphmind fn <name>", "→".cyan());
    println!("  {} graphmind map", "→".cyan());
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

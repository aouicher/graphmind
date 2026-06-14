use colored::Colorize;
use graphmind_api_client::is_remote_full;
use graphmind_config::config::{Feature, RemoteMode, Tier, load_config, save_config};
use graphmind_license::LicenseManager;

fn tier_label(tier: &Tier) -> &'static str {
    match tier {
        Tier::Free => "Free",
        Tier::Embeddings => "Embeddings",
        Tier::Pro => "Pro",
        Tier::Team => "Team",
    }
}

pub fn set(mode: &str) {
    let mut config = load_config();
    let manager = LicenseManager::from_config(&config);

    let new_mode = match mode {
        "off" => RemoteMode::Off,
        "embed" => {
            if !manager.has_feature(&Feature::RemoteEmbeddings) {
                eprintln!(
                    "{} Remote embed requires the Embeddings tier or higher.",
                    "Error:".red().bold()
                );
                eprintln!("  Upgrade at https://www.getgraphmind.com/pricing");
                eprintln!("  Current tier: {}", tier_label(manager.tier()));
                std::process::exit(1);
            }
            RemoteMode::Embed
        }
        "full" => {
            if !manager.has_feature(&Feature::RemoteMcp) {
                eprintln!(
                    "{} Remote full mode requires the Pro or Team tier.",
                    "Error:".red().bold()
                );
                eprintln!("  Upgrade at https://www.getgraphmind.com/pricing");
                eprintln!("  Current tier: {}", tier_label(manager.tier()));
                std::process::exit(1);
            }
            RemoteMode::Full
        }
        other => {
            eprintln!("{} Unknown mode '{}'. Use: off, embed, full", "Error:".red().bold(), other);
            std::process::exit(1);
        }
    };

    let prev_mode = std::mem::replace(&mut config.remote.mode, new_mode);

    // Transition: clear last_sync_at when leaving full so next build does a full sync
    if matches!(prev_mode, RemoteMode::Full) && !matches!(config.remote.mode, RemoteMode::Full) {
        config.remote.last_sync_at = None;
    }

    save_config(&config);

    println!("{} Remote mode set to {}", "OK".green().bold(), mode.cyan().bold());

    match config.remote.mode {
        RemoteMode::Embed => {
            println!("  Embeddings will be sent to the GraphMind server on next build.");
            println!("  Run {} to apply now.", "graphmind build".bold());
        }
        RemoteMode::Full => {
            println!("  Graph will sync to server + MCP will use remote SSE after next build.");
            println!("  Run {} to apply now.", "graphmind build".bold());
        }
        RemoteMode::Off => {
            println!("  Back to local-only mode. Local embeddings will be used on next build.");
        }
    }
}

pub fn status() {
    let config = load_config();
    let manager = LicenseManager::from_config(&config);

    let mode_str = match config.remote.mode {
        RemoteMode::Off => "off".dimmed().to_string(),
        RemoteMode::Embed => "embed".cyan().bold().to_string(),
        RemoteMode::Full => "full".green().bold().to_string(),
    };

    println!("Remote mode:  {}", mode_str);
    println!("License tier: {}", tier_label(manager.tier()).cyan());

    if let Some(ref synced_at) = config.remote.last_sync_at {
        println!("Last sync:    {}", synced_at.dimmed());
    } else if is_remote_full(&config) {
        println!("Last sync:    {} (run graphmind build)", "never".yellow());
    }

    println!();
    println!("Available modes:");
    println!("  {}   — local only (free)", "off  ".bold());
    println!("  {}   — server-side embeddings + semantic search (Embeddings tier)", "embed".bold());
    println!("  {}   — embed + graph sync + remote MCP SSE (Pro / Team tier)", "full ".bold());
}

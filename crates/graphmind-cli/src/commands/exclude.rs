use graphmind_config::{load_config, save_config, Registry, resolve_project_slug};
use colored::Colorize;

pub fn add(patterns: &[String], in_slug: Option<&str>, global: bool) {
    if global {
        let mut config = load_config();
        for p in patterns {
            if !config.global_exclude.contains(p) {
                config.global_exclude.push(p.clone());
            }
        }
        save_config(&config);
        println!(
            "{} Added {} global exclude pattern(s)",
            "OK".green().bold(),
            patterns.len()
        );
    } else {
        let slug = match resolve_project_slug(&[in_slug]) {
            Some(s) => s,
            None => {
                eprintln!(
                    "{} No project specified and none could be resolved",
                    "Error:".red().bold()
                );
                std::process::exit(1);
            }
        };

        Registry::update_project(&slug, |p| {
            for pattern in patterns {
                if !p.exclude.contains(pattern) {
                    p.exclude.push(pattern.clone());
                }
            }
        });
        println!(
            "{} Added {} exclude pattern(s) to {}",
            "OK".green().bold(),
            patterns.len(),
            slug.cyan()
        );
    }
}

pub fn remove(patterns: &[String], in_slug: Option<&str>, global: bool) {
    if global {
        let mut config = load_config();
        config.global_exclude.retain(|e| !patterns.contains(e));
        save_config(&config);
        println!(
            "{} Removed {} global exclude pattern(s)",
            "OK".green().bold(),
            patterns.len()
        );
    } else {
        let slug = match resolve_project_slug(&[in_slug]) {
            Some(s) => s,
            None => {
                eprintln!(
                    "{} No project specified and none could be resolved",
                    "Error:".red().bold()
                );
                std::process::exit(1);
            }
        };

        Registry::update_project(&slug, |p| {
            p.exclude.retain(|e| !patterns.contains(e));
        });
        println!(
            "{} Removed {} exclude pattern(s) from {}",
            "OK".green().bold(),
            patterns.len(),
            slug.cyan()
        );
    }
}

pub fn list(in_slug: Option<&str>, global: bool) {
    let config = load_config();

    if global {
        if config.global_exclude.is_empty() {
            println!("{}", "No global exclude patterns.".dimmed());
        } else {
            println!("{} Global exclude patterns:", ">>".cyan().bold());
            for p in &config.global_exclude {
                println!("  {p}");
            }
        }
        return;
    }

    let slug = match resolve_project_slug(&[in_slug]) {
        Some(s) => s,
        None => {
            eprintln!(
                "{} No project specified and none could be resolved",
                "Error:".red().bold()
            );
            std::process::exit(1);
        }
    };

    let project = match Registry::get(&slug) {
        Some(p) => p,
        None => {
            eprintln!("{} Project {} not found", "Error:".red().bold(), slug);
            std::process::exit(1);
        }
    };

    if project.exclude.is_empty() {
        println!("{} No exclude patterns for {}", "!".yellow(), slug);
    } else {
        println!("{} Exclude patterns for {}:", ">>".cyan().bold(), slug.cyan());
        for p in &project.exclude {
            println!("  {p}");
        }
    }

    if !config.global_exclude.is_empty() {
        println!("\n{} Global exclude patterns (also applied):", ">>".cyan().bold());
        for p in &config.global_exclude {
            println!("  {p}");
        }
    }
}

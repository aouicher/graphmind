use crate::config::Registry;
use crate::resolve::resolve_project_slug;
use colored::Colorize;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;

const POST_COMMIT_HOOK: &str = "#!/bin/sh\n# graphmind: auto-rebuild\ngraphmind build 2>/dev/null &\n";

const PRE_PUSH_HOOK: &str = "#!/bin/sh\n# graphmind: diff-impact\necho \"\"\necho \"  graphmind diff-impact:\"\ngraphmind diff-impact 2>/dev/null\necho \"\"\n";

fn install_hook(hooks_dir: &Path, name: &str, content: &str) -> bool {
    let hook_path = hooks_dir.join(name);

    if hook_path.exists() {
        let existing = fs::read_to_string(&hook_path).unwrap_or_default();
        if existing.contains("graphmind") {
            println!("  {}: already installed", name.dimmed());
            return false;
        }
        fs::write(&hook_path, format!("{existing}\n{content}")).ok();
    } else {
        fs::write(&hook_path, content).ok();
    }

    fs::set_permissions(&hook_path, fs::Permissions::from_mode(0o755)).ok();
    true
}

pub fn install(slug: Option<&str>) {
    let slug = match resolve_project_slug(&[slug]) {
        Some(s) => s,
        None => {
            eprintln!("{} Not in a registered project.", "Error:".red().bold());
            std::process::exit(1);
        }
    };

    let project = match Registry::get(&slug) {
        Some(p) => p,
        None => {
            eprintln!("{} Project \"{}\" not found.", "Error:".red().bold(), slug);
            std::process::exit(1);
        }
    };

    let git_dir = Path::new(&project.path).join(".git");
    if !git_dir.exists() {
        eprintln!("{} No .git directory in {}", "Error:".red().bold(), project.path);
        std::process::exit(1);
    }

    let hooks_dir = git_dir.join("hooks");
    fs::create_dir_all(&hooks_dir).ok();

    let mut installed = 0;
    if install_hook(&hooks_dir, "post-commit", POST_COMMIT_HOOK) {
        installed += 1;
    }
    if install_hook(&hooks_dir, "pre-push", PRE_PUSH_HOOK) {
        installed += 1;
    }

    if installed > 0 {
        println!(
            "{} Installed {} hook(s) for \"{}\"",
            "OK".green().bold(),
            installed,
            slug
        );
    } else {
        println!("{}", "All hooks already installed.".dimmed());
    }
}

pub fn uninstall(slug: Option<&str>) {
    let slug = match resolve_project_slug(&[slug]) {
        Some(s) => s,
        None => {
            eprintln!("{} Not in a registered project.", "Error:".red().bold());
            std::process::exit(1);
        }
    };

    let project = match Registry::get(&slug) {
        Some(p) => p,
        None => {
            eprintln!("{} Project \"{}\" not found.", "Error:".red().bold(), slug);
            std::process::exit(1);
        }
    };

    let hooks_dir = Path::new(&project.path).join(".git").join("hooks");
    let mut removed = 0;

    for name in &["post-commit", "pre-push"] {
        let hook_path = hooks_dir.join(name);
        if !hook_path.exists() {
            continue;
        }

        let content = match fs::read_to_string(&hook_path) {
            Ok(c) => c,
            Err(_) => continue,
        };

        if !content.contains("graphmind") {
            continue;
        }

        let filtered: Vec<&str> = content
            .lines()
            .filter(|l| !l.contains("graphmind"))
            .collect();

        let remaining = filtered
            .iter()
            .filter(|l| !l.trim().is_empty() && l.trim() != "#!/bin/sh")
            .count();

        if remaining == 0 {
            fs::remove_file(&hook_path).ok();
        } else {
            fs::write(&hook_path, filtered.join("\n")).ok();
        }
        removed += 1;
    }

    if removed > 0 {
        println!(
            "{} Removed graphmind hooks from \"{}\"",
            "OK".green().bold(),
            slug
        );
    } else {
        println!("{}", "No graphmind hooks found.".dimmed());
    }
}

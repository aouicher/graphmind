use graphmind_config::{load_config, save_config, Registry};
use serde::Serialize;
use std::path::PathBuf;

#[tauri::command]
pub fn get_app_version() -> String {
    let version = env!("CARGO_PKG_VERSION");
    let hash = env!("GIT_SHORT_HASH");
    if hash.is_empty() {
        version.to_string()
    } else {
        format!("{} ({})", version, hash)
    }
}

#[derive(Serialize)]
pub struct ExcludeSettings {
    pub global: Vec<String>,
    pub project: Vec<String>,
}

#[tauri::command]
pub fn get_excludes(slug: Option<String>) -> ExcludeSettings {
    let config = load_config();
    let project = slug
        .and_then(|s| config.projects.get(&s).cloned())
        .map(|p| p.exclude)
        .unwrap_or_default();
    ExcludeSettings {
        global: config.global_exclude,
        project,
    }
}

#[tauri::command]
pub fn set_global_excludes(excludes: Vec<String>) {
    let mut config = load_config();
    config.global_exclude = excludes;
    save_config(&config);
}

#[tauri::command]
pub fn set_project_excludes(slug: String, excludes: Vec<String>) {
    Registry::update_project(&slug, |p| {
        p.exclude = excludes.clone();
    });
}

fn hook_path() -> PathBuf {
    dirs::home_dir()
        .expect("Could not find home directory")
        .join(".claude")
        .join("hooks")
        .join("graphmind-search.sh")
}

#[tauri::command]
pub fn get_hook_status() -> bool {
    hook_path().exists()
}

#[tauri::command]
pub fn install_claude_hook() -> Result<(), String> {
    let output = std::process::Command::new("graphmind")
        .args(["install", "hook-claude"])
        .output()
        .map_err(|e| format!("Failed to run graphmind: {}", e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("Install hook failed: {}", stderr));
    }
    Ok(())
}

#[tauri::command]
pub fn uninstall_claude_hook() -> Result<(), String> {
    let output = std::process::Command::new("graphmind")
        .args(["uninstall", "hook-claude"])
        .output()
        .map_err(|e| format!("Failed to run graphmind: {}", e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("Uninstall hook failed: {}", stderr));
    }
    Ok(())
}

#[tauri::command]
pub fn get_git_hook_status(slug: Option<String>) -> bool {
    let slug = slug.or_else(|| {
        Registry::list().first().map(|p| p.slug.clone())
    });
    let Some(slug) = slug else { return false };
    let project = Registry::get(&slug);
    let Some(project) = project else { return false };
    let hooks_dir = std::path::Path::new(&project.path).join(".git/hooks");
    hooks_dir.join("post-commit").exists()
        && std::fs::read_to_string(hooks_dir.join("post-commit"))
            .map(|s| s.contains("graphmind"))
            .unwrap_or(false)
}

#[tauri::command]
pub fn install_git_hook(slug: Option<String>) -> Result<(), String> {
    let mut cmd = std::process::Command::new("graphmind");
    cmd.args(["install", "hook-git"]);
    if let Some(s) = &slug {
        cmd.arg(s);
    }
    let output = cmd.output().map_err(|e| format!("Failed to run graphmind: {}", e))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("Install git hook failed: {}", stderr));
    }
    Ok(())
}

#[tauri::command]
pub fn uninstall_git_hook(slug: Option<String>) -> Result<(), String> {
    let mut cmd = std::process::Command::new("graphmind");
    cmd.args(["uninstall", "hook-git"]);
    if let Some(s) = &slug {
        cmd.arg(s);
    }
    let output = cmd.output().map_err(|e| format!("Failed to run graphmind: {}", e))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("Uninstall git hook failed: {}", stderr));
    }
    Ok(())
}

#[tauri::command]
pub fn install_skill() -> Result<(), String> {
    let output = std::process::Command::new("graphmind")
        .args(["install", "skill"])
        .output()
        .map_err(|e| format!("Failed to run graphmind: {}", e))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("Install skill failed: {}", stderr));
    }
    Ok(())
}

#[tauri::command]
pub fn get_skill_status() -> bool {
    let skill_path = dirs::home_dir()
        .map(|h| h.join(".claude/skills/graphmind/SKILL.md"));
    skill_path.map(|p| p.exists()).unwrap_or(false)
}

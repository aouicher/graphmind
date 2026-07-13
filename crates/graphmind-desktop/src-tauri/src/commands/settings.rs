use graphmind_config::{load_config, paths, save_config, Registry};
use serde::{Deserialize, Serialize};
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

fn hooks_dir() -> PathBuf {
    dirs::home_dir()
        .expect("Could not find home directory")
        .join(".claude")
        .join("hooks")
}

#[tauri::command]
pub fn get_hook_status() -> bool {
    // All 5 hooks must be present
    let dir = hooks_dir();
    ["graphmind-search.sh", "graphmind-session.sh", "graphmind-prompt.sh", "graphmind-post.sh", "graphmind-stop.sh"]
        .iter()
        .all(|f| dir.join(f).exists())
}

#[tauri::command]
pub fn install_claude_hook() -> Result<(), String> {
    let bin = super::updater::find_graphmind_binary();
    let output = std::process::Command::new(&bin)
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
    let bin = super::updater::find_graphmind_binary();
    let output = std::process::Command::new(&bin)
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
    let bin = super::updater::find_graphmind_binary();
    let mut cmd = std::process::Command::new(&bin);
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
    let bin = super::updater::find_graphmind_binary();
    let mut cmd = std::process::Command::new(&bin);
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
    let bin = super::updater::find_graphmind_binary();
    let output = std::process::Command::new(&bin)
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

#[tauri::command]
pub fn get_claude_md_status() -> bool {
    let claude_md_path = dirs::home_dir()
        .map(|h| h.join(".claude/CLAUDE.md"));
    claude_md_path
        .and_then(|p| std::fs::read_to_string(p).ok())
        .map(|s| s.contains("<!-- GM:START -->"))
        .unwrap_or(false)
}

// ---------------------------------------------------------------------------
// Embedding settings
// ---------------------------------------------------------------------------

#[derive(Serialize)]
pub struct EmbeddingSettings {
    pub mode: String,
    pub model: Option<String>,
    pub openai_base_url: Option<String>,
    pub openai_key: Option<String>,
    pub voyage_key: Option<String>,
}

#[tauri::command]
pub fn get_embedding_settings() -> EmbeddingSettings {
    let config = load_config();
    let emb = &config.embedding;
    EmbeddingSettings {
        mode: format!("{:?}", emb.mode).to_lowercase(),
        model: emb.model.clone(),
        openai_base_url: emb.openai_base_url.clone(),
        openai_key: emb.api_keys.openai.as_deref().map(mask_key),
        voyage_key: emb.api_keys.voyage.as_deref().map(mask_key),
    }
}

fn mask_key(key: &str) -> String {
    if key.len() <= 8 {
        "****".to_string()
    } else {
        format!("{}...{}", &key[..4], &key[key.len() - 4..])
    }
}

#[derive(Deserialize)]
pub struct EmbeddingSettingsInput {
    pub mode: String,
    pub model: Option<String>,
    pub openai_base_url: Option<String>,
    pub openai_key: Option<String>,
    pub voyage_key: Option<String>,
}

#[derive(Serialize)]
pub struct EmbeddingSettingsResult {
    pub projects_needing_embedding: Vec<String>,
}

#[tauri::command]
pub fn set_embedding_settings(settings: EmbeddingSettingsInput) -> Result<EmbeddingSettingsResult, String> {
    use graphmind_config::config::EmbeddingMode;

    let mut config = load_config();
    let new_mode = match settings.mode.as_str() {
        "local" => EmbeddingMode::Local,
        "openai" => EmbeddingMode::Openai,
        "voyage" => EmbeddingMode::Voyage,
        _ => EmbeddingMode::Disabled,
    };

    config.embedding.mode = new_mode.clone();
    config.embedding.model = settings.model.filter(|s| !s.is_empty());
    config.embedding.openai_base_url = settings.openai_base_url.filter(|s| !s.is_empty());

    if let Some(key) = settings.openai_key {
        if !key.is_empty() && !key.contains("...") {
            config.embedding.api_keys.openai = Some(key);
        }
    }
    if let Some(key) = settings.voyage_key {
        if !key.is_empty() && !key.contains("...") {
            config.embedding.api_keys.voyage = Some(key);
        }
    }

    save_config(&config);

    let mut projects_needing_embedding = Vec::new();
    if new_mode != EmbeddingMode::Disabled {
        for project in Registry::list() {
            let graph_db = paths::graph_db_path(&project.slug);
            if graph_db.exists() {
                let emb_db = paths::embedding_db_path(&project.slug);
                if !emb_db.exists() {
                    projects_needing_embedding.push(project.slug.clone());
                }
            }
        }
    }

    Ok(EmbeddingSettingsResult { projects_needing_embedding })
}

// ---------------------------------------------------------------------------
// Startup settings
// ---------------------------------------------------------------------------

#[derive(Serialize)]
pub struct StartupSettings {
    pub launch_at_login: bool,
    pub build_all_on_startup: bool,
}

#[tauri::command]
pub fn get_startup_settings() -> StartupSettings {
    let config = load_config();
    StartupSettings {
        launch_at_login: config.launch_at_login,
        build_all_on_startup: config.build_all_on_startup,
    }
}

#[tauri::command]
pub fn set_launch_at_login(
    enabled: bool,
    app: tauri::AppHandle,
) -> Result<(), String> {
    use tauri_plugin_autostart::ManagerExt;
    let autostart = app.autolaunch();
    if enabled {
        autostart.enable().map_err(|e| format!("autostart enable failed: {e}"))?;
    } else {
        autostart.disable().map_err(|e| format!("autostart disable failed: {e}"))?;
    }
    let mut config = load_config();
    config.launch_at_login = enabled;
    save_config(&config);
    Ok(())
}

#[tauri::command]
pub fn set_build_all_on_startup(enabled: bool) -> Result<(), String> {
    let mut config = load_config();
    config.build_all_on_startup = enabled;
    save_config(&config);
    Ok(())
}

// ---------------------------------------------------------------------------
// Remote settings
// ---------------------------------------------------------------------------

#[derive(Serialize)]
pub struct RemoteSettings {
    pub mode: String,
    pub tier: String,
    pub last_sync_at: Option<String>,
}

#[tauri::command]
pub fn get_remote_settings() -> RemoteSettings {
    use graphmind_license::LicenseManager;
    let config = load_config();
    let manager = LicenseManager::from_config(&config);
    RemoteSettings {
        mode: format!("{:?}", config.remote.mode).to_lowercase(),
        tier: format!("{:?}", manager.tier()).to_lowercase(),
        last_sync_at: config.remote.last_sync_at.clone(),
    }
}

#[tauri::command]
pub fn set_remote_mode(mode: String) -> Result<(), String> {
    use graphmind_config::config::{Feature, RemoteMode};
    use graphmind_license::LicenseManager;

    let mut config = load_config();
    let manager = LicenseManager::from_config(&config);

    let new_mode = match mode.as_str() {
        "off" => RemoteMode::Off,
        "embed" => {
            if !manager.has_feature(&Feature::RemoteEmbeddings) {
                return Err("Remote embed requires the Embeddings tier or higher.".to_string());
            }
            RemoteMode::Embed
        }
        "full" => {
            if !manager.has_feature(&Feature::RemoteMcp) {
                return Err("Remote full mode requires the Pro or Team tier.".to_string());
            }
            RemoteMode::Full
        }
        other => return Err(format!("Unknown mode '{}'. Use: off, embed, full", other)),
    };

    let prev_mode = std::mem::replace(&mut config.remote.mode, new_mode);
    if matches!(prev_mode, RemoteMode::Full) && !matches!(config.remote.mode, RemoteMode::Full) {
        config.remote.last_sync_at = None;
    }

    save_config(&config);
    Ok(())
}

#[cfg(test)]
static SETTINGS_TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

#[cfg(test)]
mod startup_settings_tests {
    use super::*;

    fn with_temp_home(f: impl FnOnce()) {
        let _lock = super::SETTINGS_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join(".graphmind")).unwrap();
        unsafe { std::env::set_var("HOME", dir.path()); }
        f();
    }

    #[test]
    fn get_startup_settings_defaults_to_false() {
        with_temp_home(|| {
            let s = get_startup_settings();
            assert!(!s.launch_at_login);
            assert!(!s.build_all_on_startup);
        });
    }

    #[test]
    fn set_build_all_on_startup_persists() {
        with_temp_home(|| {
            set_build_all_on_startup(true).unwrap();
            let s = get_startup_settings();
            assert!(s.build_all_on_startup);

            set_build_all_on_startup(false).unwrap();
            let s2 = get_startup_settings();
            assert!(!s2.build_all_on_startup);
        });
    }

    #[test]
    fn startup_settings_roundtrip_with_other_config() {
        with_temp_home(|| {
            // Ensure build_all_on_startup persists independently from other config
            set_build_all_on_startup(true).unwrap();
            let cfg = graphmind_config::load_config();
            assert!(cfg.build_all_on_startup);
            assert!(!cfg.launch_at_login); // unrelated field untouched
        });
    }
}

#[cfg(test)]
mod remote_settings_tests {
    use super::*;
    use graphmind_config::config::RemoteMode;

    fn with_temp_home(f: impl FnOnce()) {
        let _lock = super::SETTINGS_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join(".graphmind")).unwrap();
        unsafe { std::env::set_var("HOME", dir.path()); }
        f();
    }

    // ── get_remote_settings ────────────────────────────────────────────────

    #[test]
    fn get_remote_settings_defaults_to_off() {
        with_temp_home(|| {
            let s = get_remote_settings();
            assert_eq!(s.mode, "off", "default remote mode should be off");
        });
    }

    #[test]
    fn get_remote_settings_returns_free_tier_with_no_key() {
        with_temp_home(|| {
            let s = get_remote_settings();
            assert_eq!(s.tier, "free", "no license key → free tier");
        });
    }

    #[test]
    fn get_remote_settings_last_sync_at_none_by_default() {
        with_temp_home(|| {
            let s = get_remote_settings();
            assert!(s.last_sync_at.is_none(), "last_sync_at should be None by default");
        });
    }

    #[test]
    fn get_remote_settings_reflects_saved_last_sync_at() {
        with_temp_home(|| {
            // Write Embed mode + last_sync_at directly (no tier gating needed for config write)
            let mut cfg = graphmind_config::load_config();
            cfg.remote.mode = RemoteMode::Embed;
            cfg.remote.last_sync_at = Some("2026-06-14T10:00:00Z".to_string());
            graphmind_config::save_config(&cfg);

            let s = get_remote_settings();
            assert_eq!(s.mode, "embed");
            assert_eq!(s.last_sync_at.as_deref(), Some("2026-06-14T10:00:00Z"));
        });
    }

    // ── set_remote_mode ────────────────────────────────────────────────────

    #[test]
    fn set_remote_mode_off_always_succeeds() {
        with_temp_home(|| {
            let result = set_remote_mode("off".to_string());
            assert!(result.is_ok(), "set_remote_mode off should always succeed: {:?}", result.err());
            let cfg = graphmind_config::load_config();
            assert_eq!(cfg.remote.mode, RemoteMode::Off);
        });
    }

    #[test]
    fn set_remote_mode_embed_fails_on_free_tier() {
        with_temp_home(|| {
            let result = set_remote_mode("embed".to_string());
            assert!(result.is_err(), "embed mode should fail for free tier");
            let msg = result.unwrap_err();
            assert!(msg.contains("Embeddings") || msg.contains("tier"),
                "error should mention tier: {msg}");
        });
    }

    #[test]
    fn set_remote_mode_full_fails_on_free_tier() {
        with_temp_home(|| {
            let result = set_remote_mode("full".to_string());
            assert!(result.is_err(), "full mode should fail for free tier");
            let msg = result.unwrap_err();
            assert!(msg.contains("Pro") || msg.contains("Team") || msg.contains("tier"),
                "error should mention Pro/Team: {msg}");
        });
    }

    #[test]
    fn set_remote_mode_unknown_returns_error() {
        with_temp_home(|| {
            let result = set_remote_mode("invalid_mode".to_string());
            assert!(result.is_err(), "unknown mode should return error");
            let msg = result.unwrap_err();
            assert!(msg.contains("invalid_mode") || msg.contains("Unknown"),
                "error should mention mode name: {msg}");
        });
    }

    #[test]
    fn set_remote_mode_off_clears_last_sync_at_when_was_full() {
        with_temp_home(|| {
            // Manually set full mode + last_sync_at in config
            let mut cfg = graphmind_config::load_config();
            cfg.remote.mode = RemoteMode::Full;
            cfg.remote.last_sync_at = Some("2026-06-14T10:00:00Z".to_string());
            graphmind_config::save_config(&cfg);

            // set_remote_mode("off") should clear last_sync_at
            set_remote_mode("off".to_string()).unwrap();

            let loaded = graphmind_config::load_config();
            assert_eq!(loaded.remote.mode, RemoteMode::Off);
            assert!(loaded.remote.last_sync_at.is_none(),
                "last_sync_at should be cleared when leaving Full mode");
        });
    }

    #[test]
    fn set_remote_mode_off_persists_last_sync_at_when_was_embed() {
        with_temp_home(|| {
            let mut cfg = graphmind_config::load_config();
            cfg.remote.mode = RemoteMode::Embed;
            cfg.remote.last_sync_at = Some("2026-06-14T10:00:00Z".to_string());
            graphmind_config::save_config(&cfg);

            // Going from embed → off should NOT clear last_sync_at
            set_remote_mode("off".to_string()).unwrap();

            let loaded = graphmind_config::load_config();
            assert_eq!(loaded.remote.mode, RemoteMode::Off);
            assert_eq!(loaded.remote.last_sync_at.as_deref(), Some("2026-06-14T10:00:00Z"),
                "last_sync_at should NOT be cleared when leaving Embed");
        });
    }

    #[test]
    fn set_remote_mode_off_idempotent() {
        with_temp_home(|| {
            set_remote_mode("off".to_string()).unwrap();
            let r2 = set_remote_mode("off".to_string());
            assert!(r2.is_ok(), "second set off should still succeed");
        });
    }

    #[test]
    fn get_remote_settings_mode_matches_after_set_off() {
        with_temp_home(|| {
            set_remote_mode("off".to_string()).unwrap();
            let s = get_remote_settings();
            assert_eq!(s.mode, "off");
        });
    }
}

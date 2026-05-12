use graphmind_config::{load_config, save_config};
use graphmind_license::LicenseManager;
use serde::Serialize;

#[derive(Serialize)]
pub struct LicenseStatus {
    pub display: String,
    pub tier: String,
    pub is_expired: bool,
}

#[tauri::command]
pub fn get_license_status() -> LicenseStatus {
    let config = load_config();
    let manager = LicenseManager::from_config(&config);
    LicenseStatus {
        display: manager.status_display(),
        tier: format!("{:?}", manager.tier()).to_lowercase(),
        is_expired: manager.is_expired(),
    }
}

#[tauri::command]
pub fn activate_license(key: String) -> Result<LicenseStatus, String> {
    if !key.starts_with("gm_live_") && !key.starts_with("gm_test_") {
        return Err("Clé invalide. Format attendu : gm_live_... ou gm_test_...".to_string());
    }

    let mut config = load_config();
    config.license.key = Some(key);
    config.license.last_validated_at = Some(
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs(),
    );

    let manager = LicenseManager::from_config(&config);

    if manager.is_expired() {
        return Err("Cette licence est expirée.".to_string());
    }

    save_config(&config);

    Ok(LicenseStatus {
        display: manager.status_display(),
        tier: format!("{:?}", manager.tier()).to_lowercase(),
        is_expired: false,
    })
}

#[tauri::command]
pub fn open_upgrade_page(app: tauri::AppHandle) -> Result<(), String> {
    use tauri_plugin_shell::ShellExt;
    #[allow(deprecated)]
    app.shell()
        .open("https://getgraphmind.com/pricing", None)
        .map_err(|e| format!("Impossible d'ouvrir le navigateur: {e}"))
}

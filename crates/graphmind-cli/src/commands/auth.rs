use graphmind_config::{load_config, save_config};
use graphmind_license::LicenseManager;

const KEY_PREFIX_LIVE: &str = "gm_live_";
const KEY_PREFIX_TEST: &str = "gm_test_";

pub fn login(key: &str) {
    if !key.starts_with(KEY_PREFIX_LIVE) && !key.starts_with(KEY_PREFIX_TEST) {
        eprintln!("Erreur : clé invalide. Format attendu : gm_live_... ou gm_test_...");
        std::process::exit(1);
    }

    let mut config = load_config();
    config.license.key = Some(key.to_string());
    config.license.last_validated_at = Some(
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs(),
    );

    let manager = LicenseManager::from_config(&config);

    if manager.is_expired() {
        eprintln!("Erreur : cette licence est expirée.");
        std::process::exit(1);
    }

    save_config(&config);

    println!("{}", manager.status_display());
}

pub fn status() {
    let config = load_config();
    let manager = LicenseManager::from_config(&config);
    println!("{}", manager.status_display());
}

pub fn logout() {
    let mut config = load_config();
    config.license.key = None;
    config.license.last_validated_at = None;
    save_config(&config);
    println!("Licence supprimée. Retour en mode Free.");
}

use graphmind_config::{load_config, save_config};
use graphmind_license::LicenseManager;

const KEY_PREFIX_LIVE: &str = "gm_live_";
const KEY_PREFIX_TEST: &str = "gm_test_";
const SERVER_URL_LIVE: &str = "https://graphmind-server.fly.dev";
const SERVER_URL_TEST: &str = "https://graphmind-server-staging.fly.dev";

pub fn login(key: &str) {
    if !key.starts_with(KEY_PREFIX_LIVE) && !key.starts_with(KEY_PREFIX_TEST) {
        eprintln!("Error: invalid key. Expected format: gm_live_... or gm_test_...");
        std::process::exit(1);
    }

    let server_url = if key.starts_with(KEY_PREFIX_TEST) { SERVER_URL_TEST } else { SERVER_URL_LIVE };

    // Exchange raw key for signed JWT from server
    let client = reqwest::blocking::Client::new();
    let resp = client
        .post(format!("{server_url}/v1/auth/token"))
        .header("Authorization", format!("Bearer {key}"))
        .send();

    let jwt = match resp {
        Err(e) => {
            eprintln!("Error: could not reach GraphMind server: {e}");
            std::process::exit(1);
        }
        Ok(r) if !r.status().is_success() => {
            eprintln!("Error: invalid or expired key (server returned {})", r.status());
            std::process::exit(1);
        }
        Ok(r) => {
            #[derive(serde::Deserialize)]
            struct TokenResponse { token: String }
            match r.json::<TokenResponse>() {
                Ok(t) => t.token,
                Err(e) => {
                    eprintln!("Error: unexpected server response: {e}");
                    std::process::exit(1);
                }
            }
        }
    };

    let mut config = load_config();
    // Store the JWT with prefix so LicenseManager can strip it in decode_key
    config.license.key = Some(format!("{KEY_PREFIX_LIVE}{jwt}"));
    config.license.last_validated_at = Some(
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs(),
    );

    let manager = LicenseManager::from_config(&config);

    if manager.is_expired() {
        eprintln!("Error: this license has expired.");
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
    println!("License removed. Back to Free mode.");
}

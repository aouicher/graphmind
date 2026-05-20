use graphmind_config::{load_config, save_config};
use graphmind_license::{LicenseManager, fingerprint::device_fingerprint};

const KEY_PREFIX_LIVE: &str = "gm_live_";
const KEY_PREFIX_TEST: &str = "gm_test_";
const SERVER_URL_LIVE: &str = "https://graphmind-server.fly.dev";
const SERVER_URL_TEST: &str = "https://graphmind-server-staging.fly.dev";

fn server_url(key: &str) -> &'static str {
    if key.starts_with(KEY_PREFIX_TEST) { SERVER_URL_TEST } else { SERVER_URL_LIVE }
}

pub fn login(key: &str) {
    if !key.starts_with(KEY_PREFIX_LIVE) && !key.starts_with(KEY_PREFIX_TEST) {
        eprintln!("Error: invalid key. Expected format: gm_live_... or gm_test_...");
        std::process::exit(1);
    }

    let fp = device_fingerprint();
    let client = reqwest::blocking::Client::new();
    let resp = client
        .post(format!("{}/v1/auth/token", server_url(key)))
        .header("Authorization", format!("Bearer {key}"))
        .json(&serde_json::json!({ "fingerprint": fp }))
        .send();

    let jwt = match resp {
        Err(e) => {
            eprintln!("Error: could not reach GraphMind server: {e}");
            std::process::exit(1);
        }
        Ok(r) if !r.status().is_success() => {
            let status = r.status();
            let body = r.text().unwrap_or_default();
            if status.as_u16() == 409 {
                eprintln!("Error: maximum devices reached for this license (max 2). Remove a device at https://www.getgraphmind.com/account/");
            } else {
                eprintln!("Error: invalid or expired key (server returned {status}: {body})");
            }
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
    let prefix = if key.starts_with(KEY_PREFIX_TEST) { KEY_PREFIX_TEST } else { KEY_PREFIX_LIVE };
    config.license.key = Some(format!("{prefix}{jwt}"));
    config.license.last_validated_at = Some(now_secs());

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

/// Silent background revalidation — called on every CLI invocation if 24h have passed.
/// On server error: no-op (keeps current JWT). On 401/403: clears license.
pub fn maybe_revalidate() {
    let config = load_config();
    if config.license.key.is_none() {
        return;
    }
    if !LicenseManager::needs_revalidation(&config) {
        return;
    }

    let jwt_with_prefix = match config.license.key.as_deref() {
        Some(k) => k.to_string(),
        None => return,
    };

    let (prefix, jwt) = if let Some(j) = jwt_with_prefix.strip_prefix(KEY_PREFIX_TEST) {
        (KEY_PREFIX_TEST, j.to_string())
    } else if let Some(j) = jwt_with_prefix.strip_prefix(KEY_PREFIX_LIVE) {
        (KEY_PREFIX_LIVE, j.to_string())
    } else {
        return;
    };

    let url = if prefix == KEY_PREFIX_TEST { SERVER_URL_TEST } else { SERVER_URL_LIVE };
    let fp = device_fingerprint();

    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(5))
        .build()
        .unwrap_or_default();

    let resp = client
        .post(format!("{url}/v1/auth/refresh"))
        .header("Authorization", format!("Bearer {jwt}"))
        .json(&serde_json::json!({ "fingerprint": fp }))
        .send();

    match resp {
        Err(_) => {
            // Server unreachable — keep current JWT, update timestamp to avoid hammering
            let mut config = load_config();
            config.license.last_validated_at = Some(now_secs());
            save_config(&config);
        }
        Ok(r) if r.status().is_success() => {
            #[derive(serde::Deserialize)]
            struct TokenResponse { token: String }
            if let Ok(t) = r.json::<TokenResponse>() {
                let mut config = load_config();
                config.license.key = Some(format!("{prefix}{}", t.token));
                config.license.last_validated_at = Some(now_secs());
                save_config(&config);
            }
        }
        Ok(r) if r.status().as_u16() == 401 || r.status().as_u16() == 403 => {
            // License revoked or device removed
            eprintln!("graphmind: license revoked or device unauthorized, falling back to Free.");
            let mut config = load_config();
            config.license.key = None;
            config.license.last_validated_at = None;
            save_config(&config);
        }
        Ok(_) => {
            // Other server error — keep current JWT
            let mut config = load_config();
            config.license.last_validated_at = Some(now_secs());
            save_config(&config);
        }
    }
}

fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

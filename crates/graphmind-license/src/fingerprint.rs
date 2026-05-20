use sha2::{Digest, Sha256};

pub fn device_fingerprint() -> String {
    let hostname = hostname();
    let username = username();
    let os = std::env::consts::OS;

    let mut hasher = Sha256::new();
    hasher.update(hostname.as_bytes());
    hasher.update(b"|");
    hasher.update(username.as_bytes());
    hasher.update(b"|");
    hasher.update(os.as_bytes());

    let result = hasher.finalize();
    hex::encode(&result[..8]) // 16 hex chars, stable across Rust versions
}

fn hostname() -> String {
    std::process::Command::new("hostname")
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|| "unknown".to_string())
}

fn username() -> String {
    std::env::var("USER")
        .or_else(|_| std::env::var("USERNAME"))
        .unwrap_or_else(|_| "unknown".to_string())
}

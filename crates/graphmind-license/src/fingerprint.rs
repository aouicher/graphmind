use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

pub fn device_fingerprint() -> String {
    let hostname = hostname();
    let username = username();
    let os = std::env::consts::OS;

    let mut hasher = DefaultHasher::new();
    hostname.hash(&mut hasher);
    username.hash(&mut hasher);
    os.hash(&mut hasher);

    format!("{:x}", hasher.finish())
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

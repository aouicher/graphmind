//! Unit tests for graphmind-api-client.
//! All tests are offline — no network calls are made.

#[cfg(test)]
mod api_client_tests {
    use crate::{ApiClient, ApiError, is_remote_active, is_remote_embed, is_remote_full};
    use graphmind_config::config::{GlobalConfig, LicenseConfig, RemoteConfig, RemoteMode};

    fn config_with_key(key: &str) -> GlobalConfig {
        GlobalConfig {
            license: LicenseConfig { key: Some(key.to_string()), ..Default::default() },
            ..Default::default()
        }
    }

    fn config_no_key() -> GlobalConfig {
        GlobalConfig::default()
    }

    fn config_with_mode(mode: RemoteMode) -> GlobalConfig {
        GlobalConfig {
            remote: RemoteConfig { mode, ..Default::default() },
            ..Default::default()
        }
    }

    // ── from_config ────────────────────────────────────────────────────────

    #[test]
    fn from_config_no_key_returns_no_key_err() {
        let cfg = config_no_key();
        let err = ApiClient::from_config(&cfg).err().expect("expected Err");
        assert!(matches!(err, ApiError::NoKey), "expected NoKey, got {err}");
    }

    #[test]
    fn from_config_invalid_prefix_returns_no_key_err() {
        let cfg = config_with_key("gm_invalid_prefix_abc123");
        let err = ApiClient::from_config(&cfg).err().expect("expected Err");
        assert!(matches!(err, ApiError::NoKey), "expected NoKey for bad prefix, got {err}");
    }

    #[test]
    fn from_config_live_prefix_builds_client() {
        // jwt payload doesn't matter — we just want to confirm the prefix is stripped
        let cfg = config_with_key("gm_live_some.jwt.token");
        // The client is built (no network), even if the JWT is invalid
        let result = ApiClient::from_config(&cfg);
        assert!(result.is_ok(), "gm_live_ prefix should build client, got: {:?}", result.err());
    }

    #[test]
    fn from_config_test_prefix_builds_client() {
        let cfg = config_with_key("gm_test_some.jwt.token");
        let result = ApiClient::from_config(&cfg);
        assert!(result.is_ok(), "gm_test_ prefix should build client, got: {:?}", result.err());
    }

    #[test]
    fn from_config_empty_key_returns_no_key_err() {
        let cfg = GlobalConfig {
            license: LicenseConfig { key: Some(String::new()), ..Default::default() },
            ..Default::default()
        };
        let err = ApiClient::from_config(&cfg).err().expect("expected Err");
        assert!(matches!(err, ApiError::NoKey));
    }

    // ── mcp_sse_credentials ────────────────────────────────────────────────

    #[test]
    fn mcp_sse_credentials_live_uses_prod_url() {
        let cfg = config_with_key("gm_live_some.jwt.token");
        let client = ApiClient::from_config(&cfg).unwrap();
        let (url, header) = client.mcp_sse_credentials();
        assert!(url.contains("graphmind-server.fly.dev"), "expected prod URL, got {url}");
        assert!(!url.contains("staging"), "prod URL should not contain staging");
        assert!(header.starts_with("Bearer "), "header should start with Bearer");
        assert!(header.contains("some.jwt.token"), "header should contain the raw JWT");
    }

    #[test]
    fn mcp_sse_credentials_test_uses_staging_url() {
        let cfg = config_with_key("gm_test_some.jwt.token");
        let client = ApiClient::from_config(&cfg).unwrap();
        let (url, header) = client.mcp_sse_credentials();
        assert!(url.contains("staging"), "expected staging URL, got {url}");
        assert!(header.starts_with("Bearer "), "header should start with Bearer");
    }

    #[test]
    fn mcp_sse_credentials_bearer_does_not_contain_prefix() {
        let cfg = config_with_key("gm_live_my.real.jwt");
        let client = ApiClient::from_config(&cfg).unwrap();
        let (_, header) = client.mcp_sse_credentials();
        assert!(!header.contains("gm_live_"), "Bearer header must not contain gm_live_ prefix");
        assert!(header.contains("my.real.jwt"), "Bearer header must contain raw JWT");
    }

    // ── helper functions ───────────────────────────────────────────────────

    #[test]
    fn is_remote_active_off_returns_false() {
        let cfg = config_with_mode(RemoteMode::Off);
        assert!(!is_remote_active(&cfg));
    }

    #[test]
    fn is_remote_active_embed_returns_true() {
        let cfg = config_with_mode(RemoteMode::Embed);
        assert!(is_remote_active(&cfg));
    }

    #[test]
    fn is_remote_active_full_returns_true() {
        let cfg = config_with_mode(RemoteMode::Full);
        assert!(is_remote_active(&cfg));
    }

    #[test]
    fn is_remote_embed_off_returns_false() {
        let cfg = config_with_mode(RemoteMode::Off);
        assert!(!is_remote_embed(&cfg));
    }

    #[test]
    fn is_remote_embed_embed_returns_true() {
        let cfg = config_with_mode(RemoteMode::Embed);
        assert!(is_remote_embed(&cfg));
    }

    #[test]
    fn is_remote_embed_full_returns_true() {
        // Full mode also does embed
        let cfg = config_with_mode(RemoteMode::Full);
        assert!(is_remote_embed(&cfg));
    }

    #[test]
    fn is_remote_full_off_returns_false() {
        let cfg = config_with_mode(RemoteMode::Off);
        assert!(!is_remote_full(&cfg));
    }

    #[test]
    fn is_remote_full_embed_returns_false() {
        let cfg = config_with_mode(RemoteMode::Embed);
        assert!(!is_remote_full(&cfg));
    }

    #[test]
    fn is_remote_full_full_returns_true() {
        let cfg = config_with_mode(RemoteMode::Full);
        assert!(is_remote_full(&cfg));
    }

    // ── ApiError Display ───────────────────────────────────────────────────

    #[test]
    fn api_error_no_key_display() {
        let msg = format!("{}", ApiError::NoKey);
        assert!(msg.contains("auth login"), "NoKey message should suggest auth login: {msg}");
    }

    #[test]
    fn api_error_remote_disabled_display() {
        let msg = format!("{}", ApiError::RemoteDisabled);
        assert!(msg.contains("remote.mode"), "RemoteDisabled should mention remote.mode: {msg}");
    }

    #[test]
    fn api_error_insufficient_tier_display() {
        let msg = format!("{}", ApiError::InsufficientTier {
            need: "pro".to_string(),
            have: "free".to_string(),
        });
        assert!(msg.contains("pro"), "InsufficientTier should mention required tier: {msg}");
        assert!(msg.contains("free"), "InsufficientTier should mention current tier: {msg}");
    }

    #[test]
    fn api_error_server_error_display() {
        let msg = format!("{}", ApiError::ServerError { status: 500, body: "oops".to_string() });
        assert!(msg.contains("500"), "ServerError should mention status code: {msg}");
        assert!(msg.contains("oops"), "ServerError should include body: {msg}");
    }
}

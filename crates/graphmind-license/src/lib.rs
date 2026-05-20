pub mod fingerprint;

use graphmind_config::{Feature, GlobalConfig, Tier};
use jsonwebtoken::{Algorithm, DecodingKey, Validation, decode};
use serde::{Deserialize, Serialize};

const PUBLIC_KEY_PEM: &str = "-----BEGIN PUBLIC KEY-----
MIIBIjANBgkqhkiG9w0BAQEFAAOCAQ8AMIIBCgKCAQEAweI8a2n6BjsFtXHKX41a
tGalRvgxa87I70FQSo6myaOf3AR4Rff4IMe8ik5kf9Ze+FOtmyt40OtUTSDJmz4E
LQknKz1JwylOaxwIPhlbESxE4O5DyQlJnsQtHAtDHIqTr0V3QeUIrUQTcusW8fJ/
SbXSkavh6y737zdAmKo7cQ5VjiP2kp6D8ZM1WkBLBztumOGCYh9Wobyijxtz8c92
RJ2EMYR12XH2W6kOlY8d1Bv/KFTAwQL6FE2YlHA3Abc8ZT4S40vUTG18vTh1GjjT
YCP20i6kMkhiPXAXyYGNYSOy0JZkq9rwD65HugzGZPxqTzEdxUkuzHnIhYaFryXf
wQIDAQAB
-----END PUBLIC KEY-----";

const KEY_PREFIX_LIVE: &str = "gm_live_";
const KEY_PREFIX_TEST: &str = "gm_test_";

pub const REVALIDATION_INTERVAL_SECS: u64 = 86400; // 24h

#[derive(Debug, Serialize, Deserialize)]
struct LicenseClaims {
    sub: String,           // userId
    email: String,
    tier: Tier,
    features: Vec<String>,
    fingerprint: String,
    exp: u64,
    iat: u64,
}

pub struct LicenseManager {
    tier: Tier,
    email: Option<String>,
    expires_at: Option<u64>,
}

impl LicenseManager {
    /// Builds a LicenseManager from config. Falls back to Free on any error — never panics.
    pub fn from_config(config: &GlobalConfig) -> Self {
        let Some(key) = config.license.key.as_deref() else {
            return Self::free();
        };

        match Self::decode_key(key) {
            Ok(manager) => manager,
            Err(e) => {
                eprintln!("graphmind: invalid or expired license ({e}), falling back to Free");
                Self::free()
            }
        }
    }

    fn free() -> Self {
        Self { tier: Tier::Free, email: None, expires_at: None }
    }

    fn decode_key(raw_key: &str) -> Result<Self, String> {
        let jwt = if let Some(j) = raw_key.strip_prefix(KEY_PREFIX_LIVE) {
            j
        } else if let Some(j) = raw_key.strip_prefix(KEY_PREFIX_TEST) {
            j
        } else {
            return Err("invalid key prefix".to_string());
        };

        let key = DecodingKey::from_rsa_pem(PUBLIC_KEY_PEM.as_bytes())
            .map_err(|e| format!("invalid public key: {e}"))?;
        let mut validation = Validation::new(Algorithm::RS256);
        validation.set_issuer(&["graphmind-server"]);

        let data = decode::<LicenseClaims>(jwt, &key, &validation)
            .map_err(|e| format!("invalid JWT: {e}"))?;

        // Verify device fingerprint
        let local_fp = fingerprint::device_fingerprint();
        if data.claims.fingerprint != local_fp {
            return Err(format!(
                "license is bound to a different device (expected {}, got {}). \
                 This license can only be used on registered devices.",
                &data.claims.fingerprint[..8],
                &local_fp[..8]
            ));
        }

        Ok(Self {
            tier: data.claims.tier,
            email: Some(data.claims.email),
            expires_at: Some(data.claims.exp),
        })
    }

    pub fn tier(&self) -> &Tier {
        &self.tier
    }

    pub fn has_feature(&self, feature: &Feature) -> bool {
        match feature {
            Feature::LocalGraph | Feature::LocalMcp | Feature::LocalMemory | Feature::LocalEmbeddings => true,
            Feature::RemoteEmbeddings | Feature::SemanticSearch => {
                matches!(self.tier, Tier::Embeddings | Tier::Pro | Tier::Team)
            }
            Feature::RemoteApi | Feature::RemoteMcp => {
                matches!(self.tier, Tier::Pro | Tier::Team)
            }
            Feature::TeamSync | Feature::TeamMemories => {
                matches!(self.tier, Tier::Team)
            }
        }
    }

    pub fn is_expired(&self) -> bool {
        let Some(exp) = self.expires_at else { return false };
        if exp == 0 { return false; }
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        exp < now
    }

    pub fn needs_revalidation(config: &GlobalConfig) -> bool {
        let Some(last) = config.license.last_validated_at else { return true };
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        now.saturating_sub(last) >= REVALIDATION_INTERVAL_SECS
    }

    pub fn status_display(&self) -> String {
        match &self.tier {
            Tier::Free => {
                [
                    "Current plan: Free (local)",
                    "",
                    "Available features:",
                    "✓ Unlimited local graph",
                    "✓ 25 MCP tools",
                    "✓ Local memory store",
                    "✓ Local embeddings (minilm)",
                    "",
                    "→ Upgrade: https://www.getgraphmind.com/pricing/",
                ]
                .join("\n")
            }
            tier => {
                let tier_label = match tier {
                    Tier::Embeddings => "Embeddings",
                    Tier::Pro => "Pro",
                    Tier::Team => "Team",
                    Tier::Free => unreachable!(),
                };
                let email = self.email.as_deref().unwrap_or("—");
                let expires = self.expires_at.map(format_timestamp).unwrap_or_else(|| "—".to_string());

                let mut lines = vec![
                    format!("Current plan: {tier_label}"),
                    format!("Email       : {email}"),
                    format!("Expires     : {expires}"),
                    String::new(),
                    "Active features:".to_string(),
                    "✓ Everything in Free".to_string(),
                ];

                if matches!(tier, Tier::Embeddings | Tier::Pro | Tier::Team) {
                    lines.push("✓ Remote embeddings".to_string());
                    lines.push("✓ Semantic search".to_string());
                }
                if matches!(tier, Tier::Pro | Tier::Team) {
                    lines.push("✓ Remote API".to_string());
                    lines.push("✓ Remote MCP server".to_string());
                }
                if matches!(tier, Tier::Team) {
                    lines.push("✓ Team sync (graph + memories)".to_string());
                }

                lines.join("\n")
            }
        }
    }
}

fn format_timestamp(ts: u64) -> String {
    use std::time::UNIX_EPOCH;
    let now = std::time::SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let days_left = if ts > now { (ts - now) / 86400 } else { 0 };

    let naive = chrono::DateTime::from_timestamp(ts as i64, 0)
        .map(|dt| dt.format("%d %b %Y").to_string())
        .unwrap_or_else(|| ts.to_string());

    format!("{naive} ({days_left} days)")
}

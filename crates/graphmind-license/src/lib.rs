use graphmind_config::{Feature, GlobalConfig, Tier};
use jsonwebtoken::{Algorithm, DecodingKey, Validation, decode};
use serde::{Deserialize, Serialize};

/// Placeholder — replace with real RSA public key before deploying paid tiers.
const PUBLIC_KEY_PEM: &str = "-----BEGIN PUBLIC KEY-----
REPLACE_BEFORE_DEPLOY
-----END PUBLIC KEY-----";

const KEY_PREFIX_LIVE: &str = "gm_live_";
const KEY_PREFIX_TEST: &str = "gm_test_";

#[derive(Debug, Serialize, Deserialize)]
struct LicenseClaims {
    sub: String, // email
    tier: Tier,
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
                eprintln!("graphmind: licence invalide ou expirée ({e}), mode Free activé");
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
            return Err("préfixe de clé invalide".to_string());
        };

        // PUBLIC_KEY_PEM is a placeholder — skip real verification until deployed.
        if PUBLIC_KEY_PEM.contains("REPLACE_BEFORE_DEPLOY") {
            return Self::decode_unverified(jwt);
        }

        let key = DecodingKey::from_rsa_pem(PUBLIC_KEY_PEM.as_bytes())
            .map_err(|e| format!("clé publique invalide: {e}"))?;
        let mut validation = Validation::new(Algorithm::RS256);
        validation.set_issuer(&["graphmind"]);

        let data = decode::<LicenseClaims>(jwt, &key, &validation)
            .map_err(|e| format!("JWT invalide: {e}"))?;

        Ok(Self {
            tier: data.claims.tier,
            email: Some(data.claims.sub),
            expires_at: Some(data.claims.exp),
        })
    }

    /// Decode without signature verification — used while PUBLIC_KEY_PEM is placeholder.
    fn decode_unverified(jwt: &str) -> Result<Self, String> {
        let mut validation = Validation::new(Algorithm::RS256);
        validation.insecure_disable_signature_validation();
        validation.validate_exp = false;

        let key = DecodingKey::from_secret(b"placeholder");
        let data = decode::<LicenseClaims>(jwt, &key, &validation)
            .map_err(|e| format!("JWT malformé: {e}"))?;

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        if data.claims.exp > 0 && data.claims.exp < now {
            return Err("licence expirée".to_string());
        }

        Ok(Self {
            tier: data.claims.tier,
            email: Some(data.claims.sub),
            expires_at: Some(data.claims.exp),
        })
    }

    pub fn tier(&self) -> &Tier {
        &self.tier
    }

    pub fn has_feature(&self, feature: &Feature) -> bool {
        match feature {
            // Free — always available
            Feature::LocalGraph | Feature::LocalMcp | Feature::LocalMemory | Feature::LocalEmbeddings => true,
            // Embeddings tier
            Feature::RemoteEmbeddings | Feature::SemanticSearch => {
                matches!(self.tier, Tier::Embeddings | Tier::Pro | Tier::Team)
            }
            // Pro tier
            Feature::RemoteApi | Feature::RemoteMcp => {
                matches!(self.tier, Tier::Pro | Tier::Team)
            }
            // Team tier
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

    // Format as simple date via chrono
    let naive = chrono::DateTime::from_timestamp(ts as i64, 0)
        .map(|dt| dt.format("%d %b %Y").to_string())
        .unwrap_or_else(|| ts.to_string());

    format!("{naive} ({days_left} jours)")
}

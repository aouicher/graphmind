use graphmind_config::config::{GlobalConfig, RemoteMode};
use reqwest::blocking::Client;
use serde::{Deserialize, Serialize};
use std::time::Duration;

const SERVER_URL_LIVE: &str = "https://graphmind-server.fly.dev";
const SERVER_URL_TEST: &str = "https://graphmind-server-staging.fly.dev";
const KEY_PREFIX_TEST: &str = "gm_test_";
const KEY_PREFIX_LIVE: &str = "gm_live_";

#[derive(Debug, thiserror::Error)]
pub enum ApiError {
    #[error("No license key configured. Run: graphmind auth login <key>")]
    NoKey,
    #[error("Remote mode is off. Enable it in config: remote.mode = \"embed\" or \"full\"")]
    RemoteDisabled,
    #[error("Insufficient tier for this operation (need: {need}, have: {have})")]
    InsufficientTier { need: String, have: String },
    #[error("Server error {status}: {body}")]
    ServerError { status: u16, body: String },
    #[error("Network error: {0}")]
    Network(#[from] reqwest::Error),
    #[error("Unexpected response: {0}")]
    Parse(String),
}

/// Extracted from config.license.key ("gm_live_<jwt>" or "gm_test_<jwt>").
pub struct ApiClient {
    client: Client,
    base_url: &'static str,
    jwt: String,
}

impl ApiClient {
    /// Build from GlobalConfig. Returns Err if no key is present.
    pub fn from_config(config: &GlobalConfig) -> Result<Self, ApiError> {
        let raw = config.license.key.as_deref().ok_or(ApiError::NoKey)?;
        let (base_url, jwt) = if let Some(j) = raw.strip_prefix(KEY_PREFIX_TEST) {
            (SERVER_URL_TEST, j.to_string())
        } else if let Some(j) = raw.strip_prefix(KEY_PREFIX_LIVE) {
            (SERVER_URL_LIVE, j.to_string())
        } else {
            return Err(ApiError::NoKey);
        };

        let client = Client::builder()
            .timeout(Duration::from_secs(60))
            .build()
            .map_err(ApiError::Network)?;

        Ok(Self { client, base_url, jwt })
    }

    fn url(&self, path: &str) -> String {
        format!("{}{}", self.base_url, path)
    }

    fn auth_header(&self) -> String {
        format!("Bearer {}", self.jwt)
    }

    fn check_response(&self, resp: reqwest::blocking::Response) -> Result<reqwest::blocking::Response, ApiError> {
        let status = resp.status();
        if status.is_success() {
            return Ok(resp);
        }
        let status_u16 = status.as_u16();
        if status_u16 == 403 {
            return Err(ApiError::InsufficientTier {
                need: "embeddings or pro".into(),
                have: "current tier".into(),
            });
        }
        let body = resp.text().unwrap_or_default();
        Err(ApiError::ServerError { status: status_u16, body })
    }

    // ── Embed ────────────────────────────────────────────────────────────────

    pub fn embed_chunks(&self, project_slug: &str, chunks: Vec<EmbedChunk>) -> Result<EmbedResult, ApiError> {
        let body = serde_json::json!({ "projectSlug": project_slug, "chunks": chunks });
        let resp = self.client
            .post(self.url("/v1/embed"))
            .header("Authorization", self.auth_header())
            .json(&body)
            .send()?;
        let resp = self.check_response(resp)?;
        resp.json::<EmbedResult>().map_err(|e| ApiError::Parse(e.to_string()))
    }

    pub fn search_embeddings(&self, project_slug: &str, query: &str, limit: usize) -> Result<Vec<EmbedSearchResult>, ApiError> {
        let body = serde_json::json!({ "projectSlug": project_slug, "query": query, "limit": limit });
        let resp = self.client
            .post(self.url("/v1/embed/search"))
            .header("Authorization", self.auth_header())
            .json(&body)
            .send()?;
        let resp = self.check_response(resp)?;
        #[derive(Deserialize)]
        struct Wrapper { results: Vec<EmbedSearchResult> }
        resp.json::<Wrapper>().map(|w| w.results).map_err(|e| ApiError::Parse(e.to_string()))
    }

    // ── Graph sync ───────────────────────────────────────────────────────────

    pub fn sync_graph(&self, payload: &GraphSyncPayload) -> Result<GraphSyncResult, ApiError> {
        let resp = self.client
            .post(self.url("/v1/graph/sync"))
            .header("Authorization", self.auth_header())
            .json(payload)
            .send()?;
        let resp = self.check_response(resp)?;
        resp.json::<GraphSyncResult>().map_err(|e| ApiError::Parse(e.to_string()))
    }

    pub fn graph_status(&self) -> Result<GraphStatusResult, ApiError> {
        let resp = self.client
            .get(self.url("/v1/graph/status"))
            .header("Authorization", self.auth_header())
            .send()?;
        let resp = self.check_response(resp)?;
        resp.json::<GraphStatusResult>().map_err(|e| ApiError::Parse(e.to_string()))
    }

    // ── SSE URL (for MCP config injection) ───────────────────────────────────

    /// Returns the SSE URL and the Bearer token to inject into MCP config.
    pub fn mcp_sse_credentials(&self) -> (&'static str, String) {
        let url = if self.base_url == SERVER_URL_TEST {
            "https://graphmind-server-staging.fly.dev/v1/mcp/sse"
        } else {
            "https://graphmind-server.fly.dev/v1/mcp/sse"
        };
        (url, format!("Bearer {}", self.jwt))
    }
}

/// Returns true if remote mode is active (Embed or Full).
pub fn is_remote_active(config: &GlobalConfig) -> bool {
    config.remote.mode != RemoteMode::Off
}

/// Returns true if full remote mode is active.
pub fn is_remote_full(config: &GlobalConfig) -> bool {
    config.remote.mode == RemoteMode::Full
}

/// Returns true if embed remote mode is active (Embed or Full both need remote embed).
pub fn is_remote_embed(config: &GlobalConfig) -> bool {
    matches!(config.remote.mode, RemoteMode::Embed | RemoteMode::Full)
}

// ── Types ────────────────────────────────────────────────────────────────────

#[derive(Debug, Serialize, Deserialize)]
pub struct EmbedChunk {
    pub symbol_name: String,
    pub content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file_path: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct EmbedResult {
    pub stored: usize,
    pub skipped: usize,
}

#[derive(Debug, Deserialize)]
pub struct EmbedSearchResult {
    pub symbol_name: String,
    pub score: f32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file_path: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct GraphSyncPayload {
    pub project_slug: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub since: Option<String>,
    pub symbols: Vec<SyncSymbol>,
    pub call_edges: Vec<SyncCallEdge>,
    pub file_imports: Vec<SyncFileImport>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cross_project_links: Option<Vec<SyncCrossProjectLink>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub event_listeners: Option<Vec<SyncEventListener>>,
}

#[derive(Debug, Serialize)]
pub struct SyncSymbol {
    pub name: String,
    pub kind: String,
    pub file_path: String,
    pub line_start: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub line_end: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_code: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub signature: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub qualified_name: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct SyncCallEdge {
    pub caller_name: String,
    pub caller_file: String,
    pub callee_name: String,
    pub callee_file: String,
}

#[derive(Debug, Serialize)]
pub struct SyncFileImport {
    pub source_file: String,
    pub source_project: String,
    pub target_file: String,
    pub target_project: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub import_type: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct SyncCrossProjectLink {
    pub source_project: String,
    pub source_symbol: String,
    pub target_project: String,
    pub target_symbol: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub link_type: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct SyncEventListener {
    pub event_name: String,
    pub listener_name: String,
    pub listener_file: String,
}

#[derive(Debug, Deserialize)]
pub struct GraphSyncResult {
    pub synced: bool,
    pub project_slug: String,
    pub symbols: usize,
    pub edges: usize,
    pub version: u32,
    pub synced_at: String,
}

#[derive(Debug, Deserialize)]
pub struct GraphStatusResult {
    pub exists: bool,
    pub project_slug: Option<String>,
    pub symbols: Option<u32>,
    pub edges: Option<u32>,
    pub version: Option<u32>,
    pub synced_at: Option<String>,
}

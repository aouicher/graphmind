use graphmind_config::config::{GlobalConfig, TeamConfig};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

const TIMEOUT_SECS: u64 = 30;

pub struct GraphmindClient {
    server_url: String,
    api_key: String,
}

#[derive(Debug)]
pub enum ClientError {
    NoLicense,
    NoTeamConfig,
    Http(String),
    Parse(String),
    Timeout,
}

impl std::fmt::Display for ClientError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ClientError::NoLicense => write!(f, "Aucune clé API configurée"),
            ClientError::NoTeamConfig => write!(f, "Team non configurée — lance: graphmind team init"),
            ClientError::Http(e) => write!(f, "Erreur HTTP: {e}"),
            ClientError::Parse(e) => write!(f, "Erreur de parsing: {e}"),
            ClientError::Timeout => write!(f, "Timeout (30s)"),
        }
    }
}

impl std::error::Error for ClientError {}

impl GraphmindClient {
    pub fn from_config(config: &GlobalConfig) -> Result<Self, ClientError> {
        let api_key = config
            .license
            .key
            .clone()
            .ok_or(ClientError::NoLicense)?;
        let team_cfg: &TeamConfig = config.team.as_ref().ok_or(ClientError::NoTeamConfig)?;
        Ok(Self {
            server_url: team_cfg.server_url.trim_end_matches('/').to_string(),
            api_key,
        })
    }

    pub fn from_config_pro(config: &GlobalConfig) -> Result<Self, ClientError> {
        let api_key = config
            .license
            .key
            .clone()
            .ok_or(ClientError::NoLicense)?;
        let server_url = config
            .team
            .as_ref()
            .map(|t| t.server_url.trim_end_matches('/').to_string())
            .unwrap_or_else(|| "https://graphmind-server.fly.dev".to_string());
        Ok(Self { server_url, api_key })
    }

    fn agent(&self) -> ureq::Agent {
        ureq::AgentBuilder::new()
            .timeout(std::time::Duration::from_secs(TIMEOUT_SECS))
            .build()
    }

    fn auth_header(&self) -> String {
        format!("Bearer {}", self.api_key)
    }

    // ── Graph sync ──────────────────────────────────────────

    pub fn sync_graph(&self, payload: &GraphSyncPayload) -> Result<GraphSyncResult, ClientError> {
        let url = format!("{}/v1/graph/sync", self.server_url);
        let response = self
            .agent()
            .post(&url)
            .set("Authorization", &self.auth_header())
            .set("Content-Type", "application/json")
            .send_json(ureq::serde_json::to_value(payload).map_err(|e| ClientError::Parse(e.to_string()))?)
            .map_err(|e| map_ureq_error(e))?;
        response
            .into_json::<GraphSyncResult>()
            .map_err(|e| ClientError::Parse(e.to_string()))
    }

    pub fn graph_status(&self, project_slug: &str) -> Result<GraphStatusResult, ClientError> {
        let url = format!("{}/v1/graph/status?projectSlug={}", self.server_url, project_slug);
        let response = self
            .agent()
            .get(&url)
            .set("Authorization", &self.auth_header())
            .call()
            .map_err(|e| map_ureq_error(e))?;
        response
            .into_json::<GraphStatusResult>()
            .map_err(|e| ClientError::Parse(e.to_string()))
    }

    // ── Team memories ────────────────────────────────────────

    pub fn push_memories(&self, memories: &[MemoryPushItem]) -> Result<MemoryPushResult, ClientError> {
        let url = format!("{}/v1/team/memories/push", self.server_url);
        let body = serde_json::json!({ "memories": memories });
        let response = self
            .agent()
            .post(&url)
            .set("Authorization", &self.auth_header())
            .set("Content-Type", "application/json")
            .send_json(body)
            .map_err(|e| map_ureq_error(e))?;
        response
            .into_json::<MemoryPushResult>()
            .map_err(|e| ClientError::Parse(e.to_string()))
    }

    pub fn pull_memories(
        &self,
        since: Option<u64>,
        project_slug: Option<&str>,
    ) -> Result<MemoryPullResult, ClientError> {
        let mut url = format!("{}/v1/team/memories", self.server_url);
        let mut params: Vec<String> = Vec::new();
        if let Some(ts) = since {
            let dt = chrono::DateTime::from_timestamp(ts as i64, 0)
                .map(|d| d.to_rfc3339())
                .unwrap_or_default();
            params.push(format!("since={}", urlencoding_simple(&dt)));
        }
        if let Some(slug) = project_slug {
            params.push(format!("projectSlug={}", urlencoding_simple(slug)));
        }
        if !params.is_empty() {
            url.push('?');
            url.push_str(&params.join("&"));
        }
        let response = self
            .agent()
            .get(&url)
            .set("Authorization", &self.auth_header())
            .call()
            .map_err(|e| map_ureq_error(e))?;
        response
            .into_json::<MemoryPullResult>()
            .map_err(|e| ClientError::Parse(e.to_string()))
    }

    pub fn memory_count(&self) -> Result<u64, ClientError> {
        let url = format!("{}/v1/team/memories/count", self.server_url);
        let response = self
            .agent()
            .get(&url)
            .set("Authorization", &self.auth_header())
            .call()
            .map_err(|e| map_ureq_error(e))?;
        #[derive(Deserialize)]
        struct CountResult {
            count: u64,
        }
        let result = response
            .into_json::<CountResult>()
            .map_err(|e| ClientError::Parse(e.to_string()))?;
        Ok(result.count)
    }

    pub fn check_connectivity(&self) -> bool {
        let url = format!("{}/health", self.server_url);
        self.agent()
            .get(&url)
            .timeout(std::time::Duration::from_secs(3))
            .call()
            .is_ok()
    }
}

fn map_ureq_error(e: ureq::Error) -> ClientError {
    match e {
        ureq::Error::Status(code, _) => ClientError::Http(format!("HTTP {code}")),
        ureq::Error::Transport(t) => {
            let msg = t.to_string();
            if msg.contains("timed out") || msg.contains("timeout") {
                ClientError::Timeout
            } else {
                ClientError::Http(msg)
            }
        }
    }
}

fn urlencoding_simple(s: &str) -> String {
    s.chars()
        .flat_map(|c| match c {
            'A'..='Z' | 'a'..='z' | '0'..='9' | '-' | '_' | '.' | '~' => {
                vec![c]
            }
            ' ' => vec!['+'],
            c => {
                let encoded = format!("%{:02X}", c as u32);
                encoded.chars().collect()
            }
        })
        .collect()
}

// ── Payload / Result structs ─────────────────────────────────

#[derive(Debug, Serialize)]
pub struct GraphSyncPayload {
    pub project_slug: String,
    pub since: Option<String>,
    pub symbols: Vec<SymbolItem>,
    pub call_edges: Vec<EdgeItem>,
    pub file_imports: Vec<ImportItem>,
    pub cross_project_links: Vec<CrossLinkItem>,
    pub event_listeners: Vec<ListenerItem>,
}

#[derive(Debug, Serialize)]
pub struct SymbolItem {
    pub name: String,
    pub kind: String,
    pub file_path: String,
    pub line_start: i64,
    pub line_end: Option<i64>,
    pub source_code: Option<String>,
    pub signature: Option<String>,
    pub parent_name: Option<String>,
    pub qualified_name: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct EdgeItem {
    pub caller_name: String,
    pub caller_file: String,
    pub callee_name: String,
    pub callee_file: String,
}

#[derive(Debug, Serialize)]
pub struct ImportItem {
    pub source_file: String,
    pub source_project: String,
    pub target_file: String,
    pub target_project: String,
    pub import_type: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct CrossLinkItem {
    pub source_project: String,
    pub source_symbol: String,
    pub target_project: String,
    pub target_symbol: String,
    pub link_type: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct ListenerItem {
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
    pub symbols: Option<usize>,
    pub edges: Option<usize>,
    pub version: Option<u32>,
    pub synced_at: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct MemoryPushItem {
    pub local_id: String,
    pub project_slug: String,
    pub content: String,
    pub category: Option<String>,
    pub metadata: Option<serde_json::Value>,
    pub is_deleted: bool,
    pub updated_at: String,
}

#[derive(Debug, Deserialize)]
pub struct MemoryPushResult {
    pub accepted: usize,
    pub skipped: usize,
    pub remote_ids: HashMap<String, String>,
}

#[derive(Debug, Deserialize)]
pub struct MemoryPullItem {
    pub id: String,
    pub local_id: String,
    pub project_slug: String,
    pub content: String,
    pub category: Option<String>,
    pub metadata: Option<serde_json::Value>,
    pub is_deleted: bool,
    pub created_at: String,
    pub updated_at: String,
    pub created_by: String,
}

#[derive(Debug, Deserialize)]
pub struct MemoryPullResult {
    pub memories: Vec<MemoryPullItem>,
}

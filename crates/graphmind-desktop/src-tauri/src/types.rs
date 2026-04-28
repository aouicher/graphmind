use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectInfo {
    pub slug: String,
    pub path: String,
    pub last_build: Option<String>,
    pub languages: Vec<String>,
    pub stats: Option<GraphStats>,
    pub is_watching: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphStats {
    pub symbols: i64,
    pub edges: i64,
    pub files: i64,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BuildProgress {
    pub slug: String,
    pub phase: String,
    pub current: usize,
    pub total: usize,
    pub file: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiClient {
    pub id: String,
    pub name: String,
    pub icon: String,
    pub detected: bool,
    pub mcp_configured: bool,
    pub config_path: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CliStatus {
    pub installed: bool,
    pub path: Option<String>,
    pub version: Option<String>,
}

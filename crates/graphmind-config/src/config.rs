use crate::paths;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "snake_case")]
pub enum Tier {
    #[default]
    Free,
    Embeddings,
    Pro,
    Team,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Feature {
    // Free — always available
    LocalGraph,
    LocalMcp,
    LocalMemory,
    LocalEmbeddings,
    // Embeddings tier
    RemoteEmbeddings,
    SemanticSearch,
    // Pro tier
    RemoteApi,
    RemoteMcp,
    // Team tier
    TeamSync,
    TeamMemories,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct LicenseConfig {
    pub key: Option<String>,
    #[serde(skip)]
    pub cached_tier: Option<Tier>,
    pub last_validated_at: Option<u64>,
}

/// Bump this when hooks, skills, or MCP config format changes.
/// Users with a lower stored version get a "run graphmind setup" warning.
pub const SETUP_VERSION: u32 = 2;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpConfig {
    pub transport: String,
    pub http_port: u16,
    pub restrict_to_projects: Option<Vec<String>>,
}

impl Default for McpConfig {
    fn default() -> Self {
        Self {
            transport: "stdio".to_string(),
            http_port: 37378,
            restrict_to_projects: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DefaultsConfig {
    pub embedding_model: String,
    pub watch_debounce: u64,
    pub max_depth: u32,
    pub exclude_tests: bool,
}

impl Default for DefaultsConfig {
    fn default() -> Self {
        Self {
            embedding_model: "minilm".to_string(),
            watch_debounce: 2000,
            max_depth: 5,
            exclude_tests: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectConfig {
    pub path: String,
    pub slug: String,
    pub registered: String,
    pub last_build: Option<String>,
    pub auto_watch: bool,
    pub languages: Vec<String>,
    pub exclude: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum EmbeddingMode {
    Local,
    Openai,
    Voyage,
    #[default]
    Disabled,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ApiKeys {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub openai: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub voyage: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmbeddingConfig {
    pub mode: EmbeddingMode,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub openai_base_url: Option<String>,
    #[serde(default)]
    pub api_keys: ApiKeys,
}

impl Default for EmbeddingConfig {
    fn default() -> Self {
        Self {
            mode: EmbeddingMode::Disabled,
            model: None,
            openai_base_url: None,
            api_keys: ApiKeys::default(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GlobalConfig {
    pub version: String,
    pub projects: HashMap<String, ProjectConfig>,
    pub global_exclude: Vec<String>,
    pub defaults: DefaultsConfig,
    pub mcp: McpConfig,
    #[serde(default)]
    pub embedding: EmbeddingConfig,
    #[serde(default)]
    pub setup_version: u32,
    #[serde(default)]
    pub license: LicenseConfig,
    #[serde(default)]
    pub launch_at_login: bool,
    #[serde(default)]
    pub build_all_on_startup: bool,
}

impl Default for GlobalConfig {
    fn default() -> Self {
        Self {
            version: "1".to_string(),
            projects: HashMap::new(),
            global_exclude: Vec::new(),
            defaults: DefaultsConfig::default(),
            mcp: McpConfig::default(),
            embedding: EmbeddingConfig::default(),
            setup_version: 0,
            license: LicenseConfig::default(),
            launch_at_login: false,
            build_all_on_startup: false,
        }
    }
}

pub fn ensure_dirs() {
    let dirs = [
        paths::graphmind_dir(),
        paths::graphs_dir(),
        paths::memory_dir(),
        paths::cross_links_dir(),
        paths::sessions_dir(),
    ];
    for d in &dirs {
        fs::create_dir_all(d).ok();
    }
}

pub fn load_config() -> GlobalConfig {
    let path = paths::config_path();
    if path.exists() {
        match fs::read_to_string(&path) {
            Ok(raw) => serde_json::from_str(&raw).unwrap_or_default(),
            Err(_) => GlobalConfig::default(),
        }
    } else {
        GlobalConfig::default()
    }
}

pub fn save_config(config: &GlobalConfig) {
    ensure_dirs();
    let json = serde_json::to_string_pretty(config).unwrap_or_default();
    if let Err(e) = fs::write(paths::config_path(), json) {
        eprintln!("Warning: Failed to write config: {e}");
    }
}

pub fn slugify(input: &str) -> String {
    let name = Path::new(input)
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| input.to_string());
    name.to_lowercase()
        .replace([' ', '_'], "-")
        .chars()
        .filter(|c| c.is_alphanumeric() || *c == '-')
        .collect()
}

pub struct Registry;

impl Registry {
    pub fn register(path: &str, slug: Option<&str>, exclude: &[String]) -> ProjectConfig {
        let mut config = load_config();
        let abs_path = fs::canonicalize(path)
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_else(|_| path.to_string());
        let slug = slug
            .map(|s| s.to_string())
            .unwrap_or_else(|| slugify(&abs_path));

        let now = chrono::Utc::now().to_rfc3339();
        let project = ProjectConfig {
            path: abs_path,
            slug: slug.clone(),
            registered: now,
            last_build: None,
            auto_watch: false,
            languages: Vec::new(),
            exclude: exclude.to_vec(),
        };

        config.projects.insert(slug.clone(), project.clone());
        save_config(&config);

        fs::create_dir_all(paths::graph_dir(&slug)).ok();

        project
    }

    pub fn unregister(slug: &str) -> bool {
        let mut config = load_config();
        let removed = config.projects.remove(slug).is_some();
        if removed {
            save_config(&config);
        }
        removed
    }

    pub fn get(slug: &str) -> Option<ProjectConfig> {
        let config = load_config();
        config.projects.get(slug).cloned()
    }

    pub fn list() -> Vec<ProjectConfig> {
        let config = load_config();
        config.projects.values().cloned().collect()
    }

    pub fn find_by_path(path: &str) -> Option<ProjectConfig> {
        let config = load_config();
        let abs_path = fs::canonicalize(path)
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_else(|_| path.to_string());
        config
            .projects
            .values()
            .filter(|p| abs_path == p.path || abs_path.starts_with(&format!("{}/", p.path)))
            .max_by_key(|p| p.path.len())
            .cloned()
    }

    pub fn update_project(slug: &str, f: impl FnOnce(&mut ProjectConfig)) {
        let mut config = load_config();
        if let Some(project) = config.projects.get_mut(slug) {
            f(project);
            save_config(&config);
        }
    }

    pub fn get_config() -> GlobalConfig {
        load_config()
    }
}

#[cfg(test)]
mod startup_config_tests {
    use super::*;

    #[test]
    fn global_config_default_startup_fields_are_false() {
        let cfg = GlobalConfig::default();
        assert!(!cfg.launch_at_login);
        assert!(!cfg.build_all_on_startup);
    }

    #[test]
    fn startup_fields_serialize_deserialize() {
        let cfg = GlobalConfig {
            launch_at_login: true,
            build_all_on_startup: true,
            ..GlobalConfig::default()
        };
        let json = serde_json::to_string(&cfg).unwrap();
        let parsed: GlobalConfig = serde_json::from_str(&json).unwrap();
        assert!(parsed.launch_at_login);
        assert!(parsed.build_all_on_startup);
    }

    #[test]
    fn missing_startup_fields_deserialize_to_false() {
        let json = r#"{"version":"1","projects":{},"global_exclude":[],"defaults":{"embedding_model":"minilm","watch_debounce":2000,"max_depth":5,"exclude_tests":true},"mcp":{"transport":"stdio","http_port":37378,"restrict_to_projects":null}}"#;
        let cfg: GlobalConfig = serde_json::from_str(json).unwrap();
        assert!(!cfg.launch_at_login);
        assert!(!cfg.build_all_on_startup);
    }
}

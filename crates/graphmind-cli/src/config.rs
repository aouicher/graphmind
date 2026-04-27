use crate::paths;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::Path;

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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GlobalConfig {
    pub version: String,
    pub projects: HashMap<String, ProjectConfig>,
    pub global_exclude: Vec<String>,
    pub defaults: DefaultsConfig,
    pub mcp: McpConfig,
}

impl Default for GlobalConfig {
    fn default() -> Self {
        Self {
            version: "1".to_string(),
            projects: HashMap::new(),
            global_exclude: Vec::new(),
            defaults: DefaultsConfig::default(),
            mcp: McpConfig::default(),
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
    fs::write(paths::config_path(), json).ok();
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
    pub fn register(
        path: &str,
        slug: Option<&str>,
        exclude: &[String],
    ) -> ProjectConfig {
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

        // Ensure graph directory exists
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

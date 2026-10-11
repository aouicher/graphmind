pub mod atomic;
pub mod breaking;
pub mod claude_code;
pub mod claude_settings;
pub mod config;
pub mod git_identity;
pub mod mcp_clients;
pub mod paths;
pub mod resolve;
pub mod seed;

pub use atomic::write_atomic;
pub use breaking::{update_crosses_breaking, BREAKING_VERSIONS};
pub use config::{
    ensure_dirs, load_config, read_config, save_config, slugify, ConfigError, DefaultsConfig,
    EmbeddingConfig, EmbeddingMode, Feature, GlobalConfig, LicenseConfig, McpConfig, ProjectConfig,
    Registry, Tier, SETUP_VERSION,
};
pub use resolve::{resolve_path, resolve_project_slug};
pub use seed::{lock_build, on_worktree_linked, seed_graph, spawn_background_build};

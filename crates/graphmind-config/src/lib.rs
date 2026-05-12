pub mod breaking;
pub mod config;
pub mod paths;
pub mod resolve;

pub use breaking::{update_crosses_breaking, BREAKING_VERSIONS};
pub use config::{
    ensure_dirs, load_config, save_config, slugify, DefaultsConfig, EmbeddingConfig, EmbeddingMode,
    Feature, GlobalConfig, LicenseConfig, McpConfig, ProjectConfig, Registry, Tier, SETUP_VERSION,
};
pub use resolve::resolve_project_slug;

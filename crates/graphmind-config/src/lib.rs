pub mod config;
pub mod paths;
pub mod resolve;

pub use config::{
    ensure_dirs, load_config, save_config, slugify, DefaultsConfig, GlobalConfig, McpConfig,
    ProjectConfig, Registry, SETUP_VERSION,
};
pub use resolve::resolve_project_slug;

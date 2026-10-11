pub mod graph;
pub mod license;
pub mod indexing;
pub mod integrations;
pub mod notices;
pub mod projects;
pub mod settings;
pub mod setup;
pub mod updater;
pub mod watcher;

/// Serialises the tests that set process-wide environment variables (HOME, CLAUDE_CONFIG_DIR, PATH).
#[cfg(test)]
pub(crate) static ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

pub mod auth;
pub mod build;
pub mod claude_hook;
pub mod clean;
pub mod cross;
pub mod diff_impact;
pub mod exclude;
pub mod export;
pub mod hooks;
pub mod install_skill;
pub mod memory;
pub mod notices;
pub mod query;
pub mod register;
pub mod search;
pub mod session;
pub mod setup;
pub mod sync;
pub mod update;

/// [`graphmind_config::try_resolve_project_slug`] for CLI commands: an unknown
/// project prints the error and exits with status 1; `None` means no project was
/// given and none could be resolved from the current directory.
pub fn resolve_project_slug(candidates: &[Option<&str>]) -> Option<String> {
    graphmind_config::try_resolve_project_slug(candidates).unwrap_or_else(|e| {
        eprintln!("Error: {e}");
        std::process::exit(1);
    })
}

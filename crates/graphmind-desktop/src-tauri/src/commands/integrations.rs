//! MCP integrations card: status, install and uninstall per AI client. The config handling is
//! `graphmind_config::mcp_clients`, the same code `graphmind setup` runs, so both front-ends
//! agree on where each client's entry lives and never replace a config they can't parse.

use crate::types::AiClient;
use graphmind_config::claude_code;
use graphmind_config::claude_settings;
use graphmind_config::mcp_clients::{self, McpClient, Outcome};
use graphmind_config::paths::{find_on_path, home_dir};

/// Claude Code reads graphmind per project (`projects.<key>.mcpServers` in .claude.json, one key per
/// repository), so its card covers the entries of every registered project rather than one config
/// entry.
const CLAUDE_CODE: &str = "claude-code";

/// The single-entry client behind a card id.
fn client_for(client_id: &str) -> Result<McpClient, String> {
    match client_id {
        "claude-desktop" => Ok(McpClient::ClaudeDesktop),
        "cursor" => Ok(McpClient::Cursor),
        "opencode" => Ok(McpClient::OpenCode),
        _ => Err(format!("Unsupported client: {client_id}")),
    }
}

/// Claude Code counts as installed for configuration (graphmind's hooks target it), so the card
/// shows whether it is actually on this machine.
fn is_claude_detected() -> bool {
    home_dir().join(".claude").is_dir() || find_on_path("claude").is_some()
}

fn client_card(id: &str, icon: &str, client: McpClient) -> AiClient {
    AiClient {
        id: id.to_string(),
        name: client.label().to_string(),
        icon: icon.to_string(),
        detected: client.is_installed(),
        mcp_configured: mcp_clients::has_entry(&client),
        config_path: Some(client.config_path().to_string_lossy().to_string()),
    }
}

/// Configured when every key of the registered projects has its entry (see
/// `claude_code::claude_code_configured`).
fn claude_code_card() -> AiClient {
    AiClient {
        id: CLAUDE_CODE.to_string(),
        name: "Claude Code".to_string(),
        icon: "terminal".to_string(),
        detected: is_claude_detected(),
        mcp_configured: claude_code::claude_code_configured(&mcp_clients::graphmind_binary()),
        config_path: Some(McpClient::ClaudeCodeUser.config_path().to_string_lossy().to_string()),
    }
}

#[tauri::command]
pub fn detect_clients() -> Vec<AiClient> {
    vec![
        claude_code_card(),
        client_card("claude-desktop", "app-window", McpClient::ClaudeDesktop),
        client_card("cursor", "mouse-pointer", McpClient::Cursor),
        client_card("opencode", "code", McpClient::OpenCode),
    ]
}

// Async so a `claude mcp ...` call (up to its timeout) doesn't block the UI thread.
#[tauri::command]
pub async fn install_mcp_for_client(client_id: String) -> Result<(), String> {
    if client_id == CLAUDE_CODE {
        return run_blocking(install_claude_code).await;
    }
    let client = client_for(&client_id)?;
    run_blocking(move || install(&client)).await
}

#[tauri::command]
pub async fn uninstall_mcp_for_client(client_id: String) -> Result<(), String> {
    if client_id == CLAUDE_CODE {
        return run_blocking(uninstall_claude_code).await;
    }
    let client = client_for(&client_id)?;
    run_blocking(move || uninstall(&client)).await
}

async fn run_blocking(f: impl FnOnce() -> Result<(), String> + Send + 'static) -> Result<(), String> {
    tokio::task::spawn_blocking(f).await.map_err(|e| e.to_string())?
}

pub(crate) fn log_cli_fallback(reason: &str) {
    eprintln!("[graphmind] {reason}; writing the file directly");
}

fn install(client: &McpClient) -> Result<(), String> {
    into_result(client, mcp_clients::upsert(client, &mcp_clients::graphmind_binary(), &mut log_cli_fallback))
}

fn uninstall(client: &McpClient) -> Result<(), String> {
    into_result(client, mcp_clients::remove(client, &mut log_cli_fallback))
}

/// An entry for every key of the registered projects, the same as `graphmind init` in each. As in
/// `graphmind setup`, a user-scope entry that runs graphmind is removed once they are in place,
/// along with the old ~/.claude/settings.json entry and the `env.PATH` of issue #105; a failed
/// settings.json repair doesn't fail the install: the entries work.
pub(crate) fn install_claude_code() -> Result<(), String> {
    if claude_code::claude_code_keys().is_empty() {
        return Err("No registered project: run `graphmind init` in a project first".to_string());
    }
    let sync = claude_code::sync_claude_code_projects(&mcp_clients::graphmind_binary(), true, &mut log_cli_fallback);
    if sync.is_complete() {
        if let Err(reason) = claude_settings::repair_claude_settings(true) {
            eprintln!("[graphmind] {reason}");
        }
    }
    sync_result(sync)
}

fn sync_result(sync: claude_code::ClaudeCodeSync) -> Result<(), String> {
    match (sync.left_alone.first(), sync.user_scope) {
        (Some(reason), _) => Err(reason.clone()),
        (None, Outcome::LeftAlone(reason) | Outcome::Kept(reason)) => Err(reason),
        (None, _) => Ok(()),
    }
}

/// graphmind's entries in .claude.json (see `claude_code::uninstall_claude_code`), and its MCP
/// entry and env.PATH segment in ~/.claude/settings.json, so no graphmind entry is left for Claude
/// Code to load.
fn uninstall_claude_code() -> Result<(), String> {
    let removal = claude_code::uninstall_claude_code(&mut log_cli_fallback);
    let user_scope = into_result(&McpClient::ClaudeCodeUser, removal.user_scope);
    let settings = claude_settings::repair_claude_settings(true).map(drop);
    removal.projects.map(drop).and(user_scope).and(settings)
}

fn into_result(client: &McpClient, outcome: Outcome) -> Result<(), String> {
    match outcome {
        Outcome::Unchanged | Outcome::Written => Ok(()),
        Outcome::NotInstalled => Err(format!("{} is not installed", client.label())),
        Outcome::LeftAlone(reason) | Outcome::Kept(reason) => Err(reason),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn claude_code_card_is_not_a_single_entry_client() {
        // Its entries are per project in .claude.json, not one entry (and never the user-scope or
        // ~/.claude/settings.json one): install and uninstall handle every registered project.
        assert!(client_for(CLAUDE_CODE).is_err());
        let project = McpClient::ClaudeCodeProject("/repo/a".into());
        assert_eq!(project.config_path().file_name().unwrap(), ".claude.json");
        assert_eq!(project.label(), "Claude Code");
    }

    #[test]
    fn card_ids_map_to_shared_clients() {
        assert_eq!(client_for("claude-desktop").unwrap(), McpClient::ClaudeDesktop);
        assert_eq!(client_for("cursor").unwrap(), McpClient::Cursor);
        assert_eq!(client_for("opencode").unwrap(), McpClient::OpenCode);
        assert!(client_for("vim").is_err());
    }

    #[test]
    fn claude_code_install_fails_with_the_first_reason() {
        let sync = |left_alone: Vec<&str>, user_scope| claude_code::ClaudeCodeSync {
            keys: 1,
            written: Vec::new(),
            current: 0,
            hand_made: Vec::new(),
            moved: Vec::new(),
            left_alone: left_alone.into_iter().map(str::to_string).collect(),
            user_scope,
        };
        assert!(sync_result(sync(vec![], Outcome::Written)).is_ok());
        assert!(sync_result(sync(vec![], Outcome::Unchanged)).is_ok());
        assert_eq!(
            sync_result(sync(vec!["a is not valid JSON", "b"], Outcome::LeftAlone("kept".into()))),
            Err("a is not valid JSON".to_string())
        );
        assert_eq!(sync_result(sync(vec![], Outcome::LeftAlone("kept".into()))), Err("kept".to_string()));
    }

    #[test]
    fn outcomes_that_change_nothing_are_errors() {
        let client = McpClient::Cursor;
        assert!(into_result(&client, Outcome::Written).is_ok());
        assert!(into_result(&client, Outcome::Unchanged).is_ok());
        assert_eq!(into_result(&client, Outcome::NotInstalled), Err("Cursor is not installed".to_string()));
        assert_eq!(
            into_result(&client, Outcome::LeftAlone("x is not valid JSON".to_string())),
            Err("x is not valid JSON".to_string())
        );
    }
}

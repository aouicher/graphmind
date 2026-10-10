//! MCP integrations card: status, install and uninstall per AI client. The config handling is
//! `graphmind_config::mcp_clients`, the same code `graphmind setup` runs, so both front-ends
//! agree on where each client's entry lives and never replace a config they can't parse.

use crate::types::AiClient;
use graphmind_config::claude_settings;
use graphmind_config::mcp_clients::{self, McpClient, Outcome};
use std::path::PathBuf;

fn home_dir() -> PathBuf {
    dirs::home_dir().expect("Cannot determine home directory")
}

/// The client behind a card id.
fn client_for(client_id: &str) -> Result<McpClient, String> {
    match client_id {
        "claude-code" => Ok(McpClient::ClaudeCodeUser),
        "claude-desktop" => Ok(McpClient::ClaudeDesktop),
        "cursor" => Ok(McpClient::Cursor),
        "opencode" => Ok(McpClient::OpenCode),
        _ => Err(format!("Unsupported client: {client_id}")),
    }
}

/// Claude Code counts as installed for configuration (graphmind's hooks target it), so the card
/// shows whether it is actually on this machine.
fn is_claude_detected() -> bool {
    home_dir().join(".claude").is_dir()
        || std::process::Command::new("which")
            .arg("claude")
            .output()
            .is_ok_and(|o| o.status.success())
}

fn client_card(id: &str, icon: &str, client: McpClient) -> AiClient {
    let detected = match client {
        McpClient::ClaudeCodeUser => is_claude_detected(),
        _ => client.is_installed(),
    };
    AiClient {
        id: id.to_string(),
        name: client.label().to_string(),
        icon: icon.to_string(),
        detected,
        mcp_configured: mcp_clients::has_entry(&client),
        config_path: Some(client.config_path().to_string_lossy().to_string()),
    }
}

#[tauri::command]
pub fn detect_clients() -> Vec<AiClient> {
    vec![
        client_card("claude-code", "terminal", McpClient::ClaudeCodeUser),
        client_card("claude-desktop", "app-window", McpClient::ClaudeDesktop),
        client_card("cursor", "mouse-pointer", McpClient::Cursor),
        client_card("opencode", "code", McpClient::OpenCode),
    ]
}

// Async so a `claude mcp ...` call (up to its timeout) doesn't block the UI thread.
#[tauri::command]
pub async fn install_mcp_for_client(client_id: String) -> Result<(), String> {
    let client = client_for(&client_id)?;
    tokio::task::spawn_blocking(move || install(&client))
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn uninstall_mcp_for_client(client_id: String) -> Result<(), String> {
    let client = client_for(&client_id)?;
    tokio::task::spawn_blocking(move || uninstall(&client))
        .await
        .map_err(|e| e.to_string())?
}

fn log_cli_fallback(reason: &str) {
    eprintln!("[graphmind] {reason}; writing the file directly");
}

/// Same steps as `graphmind setup`: once the Claude Code user-scope entry is in place, the old
/// entries (~/.claude/settings.json, per-project ones in ~/.claude.json) go, along with the
/// legacy `env.PATH` of issue #105. A failed cleanup doesn't fail the install: the entry works.
fn install(client: &McpClient) -> Result<(), String> {
    let outcome = mcp_clients::upsert(client, &mcp_clients::graphmind_binary(), &mut log_cli_fallback);
    if *client == McpClient::ClaudeCodeUser && outcome.is_in_place() {
        if let Err(reason) = remove_claude_code_leftovers() {
            eprintln!("[graphmind] {reason}");
        }
    }
    into_result(client, outcome)
}

/// Removes the user-scope entry and what older versions left (settings.json entry and env.PATH,
/// per-project entries), so no graphmind entry is left for Claude Code to load.
fn uninstall(client: &McpClient) -> Result<(), String> {
    let outcome = mcp_clients::remove(client, &mut log_cli_fallback);
    let leftovers = match client {
        McpClient::ClaudeCodeUser => remove_claude_code_leftovers(),
        _ => Ok(()),
    };
    into_result(client, outcome)?;
    leftovers
}

fn remove_claude_code_leftovers() -> Result<(), String> {
    let settings = claude_settings::repair_claude_settings(true).map(drop);
    let projects = mcp_clients::remove_claude_project_entries(None).map(drop);
    settings.and(projects)
}

fn into_result(client: &McpClient, outcome: Outcome) -> Result<(), String> {
    match outcome {
        Outcome::Unchanged | Outcome::Written => Ok(()),
        Outcome::NotInstalled => Err(format!("{} is not installed", client.label())),
        Outcome::LeftAlone(reason) => Err(reason),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn claude_code_card_uses_the_user_scope_config() {
        // Where `claude mcp add -s user` writes, not ~/.claude/settings.json (which Claude Code
        // doesn't read servers from, and setup cleans up).
        let client = client_for("claude-code").unwrap();
        assert_eq!(client, McpClient::ClaudeCodeUser);
        assert_eq!(client.config_path().file_name().unwrap(), ".claude.json");
    }

    #[test]
    fn card_ids_map_to_shared_clients() {
        assert_eq!(client_for("claude-desktop").unwrap(), McpClient::ClaudeDesktop);
        assert_eq!(client_for("cursor").unwrap(), McpClient::Cursor);
        assert_eq!(client_for("opencode").unwrap(), McpClient::OpenCode);
        assert!(client_for("vim").is_err());
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

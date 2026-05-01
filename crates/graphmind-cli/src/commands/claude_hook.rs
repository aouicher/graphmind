use colored::Colorize;
use std::fs;
use std::path::PathBuf;

const HOOK_SCRIPT: &str = r#"#!/usr/bin/env bash
# graphmind Claude Code hook — reminds to use graphmind for code search
# Intercepts Grep and Bash(grep/find/rg/ag) to suggest graphmind first.

if ! command -v jq &>/dev/null; then
  exit 0
fi

if ! command -v graphmind &>/dev/null; then
  exit 0
fi

INPUT=$(cat)
TOOL_NAME="${CLAUDE_TOOL_NAME:-}"

# Check if we're in a graphmind-registered project
PROJECT_STATUS=$(graphmind status 2>/dev/null)
if [ $? -ne 0 ]; then
  exit 0
fi

should_intercept() {
  if [ "$TOOL_NAME" = "Grep" ]; then
    return 0
  fi

  if [ "$TOOL_NAME" = "Bash" ]; then
    CMD=$(echo "$INPUT" | jq -r '.tool_input.command // empty')
    if echo "$CMD" | grep -qE '^\s*(grep|rg|ag|find|fd)\b'; then
      return 0
    fi
  fi

  return 1
}

if ! should_intercept; then
  exit 0
fi

# Output additional context message
jq -n '{
  "hookSpecificOutput": {
    "hookEventName": "PreToolUse",
    "additionalContext": "⚡ graphmind is available for this project. Before grep/find, prefer:\n- `graphmind search \"<query>\"` or MCP `gm_search` for semantic symbol search\n- `graphmind fn <symbol>` or MCP `gm_fn` for function lookup with source\n- `graphmind deps <file>` or MCP `gm_deps` for file dependencies\n- `graphmind query <name>` or MCP `gm_query` for symbol resolution\nOnly fall back to grep/find if graphmind cannot answer the query (e.g., searching for string literals, config values, or non-code patterns)."
  }
}'
"#;

fn hooks_dir() -> PathBuf {
    dirs::home_dir()
        .expect("Could not find home directory")
        .join(".claude")
        .join("hooks")
}

fn hook_path() -> PathBuf {
    hooks_dir().join("graphmind-search.sh")
}

fn settings_path() -> PathBuf {
    dirs::home_dir()
        .expect("Could not find home directory")
        .join(".claude")
        .join("settings.json")
}

pub fn install_hook() {
    let dir = hooks_dir();
    fs::create_dir_all(&dir).unwrap_or_else(|e| {
        eprintln!("{} Failed to create hooks directory: {}", "Error:".red().bold(), e);
        std::process::exit(1);
    });

    let path = hook_path();
    fs::write(&path, HOOK_SCRIPT).unwrap_or_else(|e| {
        eprintln!("{} Failed to write hook script: {}", "Error:".red().bold(), e);
        std::process::exit(1);
    });

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).ok();
    }

    // Register hook in Claude Code settings.json
    if let Err(e) = register_in_settings() {
        eprintln!("{} Hook script installed but failed to register in settings: {}", "Warning:".yellow().bold(), e);
        println!("  Manually add the hook to ~/.claude/settings.json under hooks.PreToolUse");
    } else {
        println!("{} Hook registered in Claude Code settings", "OK".green().bold());
    }

    println!(
        "{} Hook installed to {}",
        "OK".green().bold(),
        path.display()
    );
}

pub fn uninstall_hook() {
    let path = hook_path();
    if path.exists() {
        fs::remove_file(&path).unwrap_or_else(|e| {
            eprintln!("{} Failed to remove hook: {}", "Error:".red().bold(), e);
            std::process::exit(1);
        });
    }

    if let Err(e) = unregister_from_settings() {
        eprintln!("{} Failed to unregister from settings: {}", "Warning:".yellow().bold(), e);
    } else {
        println!("{} Hook unregistered from Claude Code settings", "OK".green().bold());
    }

    println!("{} Hook uninstalled", "OK".green().bold());
}

pub fn hook_status() -> bool {
    hook_path().exists()
}

fn register_in_settings() -> Result<(), String> {
    let path = settings_path();
    let content = fs::read_to_string(&path).unwrap_or_else(|_| "{}".to_string());

    let mut settings: serde_json::Value = serde_json::from_str(&content)
        .map_err(|e| format!("Failed to parse settings.json: {}", e))?;

    let hooks = settings
        .as_object_mut()
        .ok_or("settings is not an object")?
        .entry("hooks")
        .or_insert_with(|| serde_json::json!({}));

    let pre_tool_use = hooks
        .as_object_mut()
        .ok_or("hooks is not an object")?
        .entry("PreToolUse")
        .or_insert_with(|| serde_json::json!([]));

    let arr = pre_tool_use
        .as_array_mut()
        .ok_or("PreToolUse is not an array")?;

    let hook_cmd = hook_path().to_string_lossy().to_string();

    // Add Grep matcher if not present
    let grep_entry = serde_json::json!({
        "matcher": "Grep",
        "hooks": [{"type": "command", "command": &hook_cmd}]
    });

    // Add Bash matcher if not present (in addition to existing Bash hooks)
    let bash_entry = serde_json::json!({
        "matcher": "Bash",
        "hooks": [{"type": "command", "command": &hook_cmd}]
    });

    let has_graphmind_grep = arr.iter().any(|entry| {
        entry.get("matcher").and_then(|m| m.as_str()) == Some("Grep")
            && entry.get("hooks").and_then(|h| h.as_array()).map_or(false, |hooks| {
                hooks.iter().any(|h| {
                    h.get("command").and_then(|c| c.as_str()).map_or(false, |c| c.contains("graphmind"))
                })
            })
    });

    let has_graphmind_bash = arr.iter().any(|entry| {
        entry.get("matcher").and_then(|m| m.as_str()) == Some("Bash")
            && entry.get("hooks").and_then(|h| h.as_array()).map_or(false, |hooks| {
                hooks.iter().any(|h| {
                    h.get("command").and_then(|c| c.as_str()).map_or(false, |c| c.contains("graphmind"))
                })
            })
    });

    if !has_graphmind_grep {
        arr.push(grep_entry);
    }

    // For Bash, we need to add our hook to the existing Bash matcher or create a new one
    if !has_graphmind_bash {
        // Check if there's an existing Bash matcher we should add to
        let existing_bash = arr.iter_mut().find(|entry| {
            entry.get("matcher").and_then(|m| m.as_str()) == Some("Bash")
                && entry.get("hooks").and_then(|h| h.as_array()).map_or(false, |hooks| {
                    !hooks.iter().any(|h| {
                        h.get("command").and_then(|c| c.as_str()).map_or(false, |c| c.contains("graphmind"))
                    })
                })
        });

        if let Some(bash_matcher) = existing_bash {
            // Add our hook to the existing Bash matcher's hooks array
            if let Some(hooks_arr) = bash_matcher.get_mut("hooks").and_then(|h| h.as_array_mut()) {
                hooks_arr.push(serde_json::json!({"type": "command", "command": &hook_cmd}));
            }
        } else {
            arr.push(bash_entry);
        }
    }

    let output = serde_json::to_string_pretty(&settings)
        .map_err(|e| format!("Failed to serialize settings: {}", e))?;

    fs::write(&path, output)
        .map_err(|e| format!("Failed to write settings.json: {}", e))?;

    Ok(())
}

fn unregister_from_settings() -> Result<(), String> {
    let path = settings_path();
    let content = fs::read_to_string(&path)
        .map_err(|e| format!("Failed to read settings.json: {}", e))?;

    let mut settings: serde_json::Value = serde_json::from_str(&content)
        .map_err(|e| format!("Failed to parse settings.json: {}", e))?;

    if let Some(hooks) = settings.get_mut("hooks").and_then(|h| h.as_object_mut()) {
        if let Some(pre_tool_use) = hooks.get_mut("PreToolUse").and_then(|p| p.as_array_mut()) {
            // Remove entries that only have graphmind hooks
            pre_tool_use.retain(|entry| {
                let hooks_arr = entry.get("hooks").and_then(|h| h.as_array());
                if let Some(hooks) = hooks_arr {
                    // If all hooks in this entry are graphmind, remove the whole entry
                    let all_graphmind = hooks.iter().all(|h| {
                        h.get("command").and_then(|c| c.as_str()).map_or(false, |c| c.contains("graphmind"))
                    });
                    if all_graphmind {
                        return false;
                    }
                }
                true
            });

            // For entries with mixed hooks, just remove the graphmind ones
            for entry in pre_tool_use.iter_mut() {
                if let Some(hooks_arr) = entry.get_mut("hooks").and_then(|h| h.as_array_mut()) {
                    hooks_arr.retain(|h| {
                        !h.get("command").and_then(|c| c.as_str()).map_or(false, |c| c.contains("graphmind"))
                    });
                }
            }
        }
    }

    let output = serde_json::to_string_pretty(&settings)
        .map_err(|e| format!("Failed to serialize settings: {}", e))?;

    fs::write(&path, output)
        .map_err(|e| format!("Failed to write settings.json: {}", e))?;

    Ok(())
}

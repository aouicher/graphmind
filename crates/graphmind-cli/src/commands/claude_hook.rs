use colored::Colorize;
use std::fs;
use std::path::PathBuf;

const PRE_TOOL_HOOK: &str = r#"#!/usr/bin/env bash
# graphmind PreToolUse hook — lightweight safety net
# Reminds Claude to use /gm skill instead of grep/find/Explore agents.
# Does NOT rewrite commands — just nudges via additionalContext.

export PATH="$HOME/.graphmind/bin:$HOME/.local/bin:/usr/local/bin:$PATH"

if ! command -v jq &>/dev/null; then exit 0; fi
if ! command -v graphmind &>/dev/null; then exit 0; fi

INPUT=$(cat)
TOOL_NAME=$(echo "$INPUT" | jq -r '.tool_name // .toolName // empty')

# Check if we're in a graphmind-registered project
graphmind status &>/dev/null || exit 0

should_nudge() {
  case "$TOOL_NAME" in
    Grep) return 0 ;;
    Bash)
      CMD=$(echo "$INPUT" | jq -r '.tool_input.command // empty')
      # Only nudge for search-like commands, not graphmind itself
      echo "$CMD" | grep -q "graphmind" && return 1
      echo "$CMD" | grep -qE '^\s*(rtk\s+)?(grep|rg|ag|find|fd)\b' && return 0
      ;;
    Agent)
      SUBAGENT=$(echo "$INPUT" | jq -r '.tool_input.subagent_type // empty')
      [ "$SUBAGENT" = "Explore" ] && return 0
      PROMPT=$(echo "$INPUT" | jq -r '.tool_input.prompt // empty')
      echo "$PROMPT" | grep -qiE 'search|find|explore|locate|where is|which file' && return 0
      ;;
  esac
  return 1
}

if ! should_nudge; then exit 0; fi

# Skip for exhaustive searches — grep is legitimately better
CMD_CHECK=$(echo "$INPUT" | jq -r '(.tool_input.command // "") + " " + (.tool_input.description // "")')
if echo "$CMD_CHECK" | grep -qiE 'all (usages|occurrences|references|instances)|find all|list all|exhaustive'; then
  exit 0
fi
if echo "$CMD_CHECK" | grep -qE '\s-[a-zA-Z]*c|\|\s*wc\s|\|\s*sort\s|\|\s*uniq\s'; then
  exit 0
fi

MSG="⚡ This project is graphmind-indexed. Use /gm <query> for code exploration (symbols, callers, deps, outlines). Grep only for string literals/config values."

jq -n --arg msg "$MSG" '{
  "hookSpecificOutput": {
    "hookEventName": "PreToolUse",
    "additionalContext": $msg
  }
}'
"#;

const SESSION_START_HOOK: &str = r#"#!/usr/bin/env bash
# graphmind SessionStart hook — full project briefing with priority memory

export PATH="$HOME/.graphmind/bin:$HOME/.local/bin:/usr/local/bin:$PATH"

if ! command -v jq &>/dev/null; then exit 0; fi
if ! command -v graphmind &>/dev/null; then exit 0; fi

# Check if we're in a graphmind-registered project
graphmind status &>/dev/null || exit 0

# Get project summary
STATUS=$(graphmind status 2>/dev/null | head -10)
MAP=$(graphmind map 2>/dev/null | head -20)

# Load priority memories (always-inject: conventions, decisions, preferences)
PRIORITY_MEM=$(graphmind memory list --priority --limit 20 2>/dev/null | head -30)

# Load recent non-priority memories
RECENT_MEM=$(graphmind memory list --limit 5 2>/dev/null | head -15)

MSG="[graphmind] Project indexed and ready. Use MCP tools (gm_search, gm_fn, gm_deps, gm_map, gm_outline, gm_query) for ALL code exploration.\n\nProject status:\n$STATUS\n\nStructure:\n$MAP"

if [ -n "$PRIORITY_MEM" ]; then
  MSG="$MSG\n\n★ Priority context (always active):\n$PRIORITY_MEM"
fi

if [ -n "$RECENT_MEM" ]; then
  MSG="$MSG\n\nRecent memories:\n$RECENT_MEM"
fi

MSG="$MSG\n\nAuto-memory is ON: save decisions/patterns/conventions via gm_memory_add. Use --priority for always-injected facts. Recall via gm_memory_search."

jq -n --arg msg "$MSG" '{
  "hookSpecificOutput": {
    "hookEventName": "SessionStart",
    "additionalContext": $msg
  }
}'
"#;

const USER_PROMPT_HOOK: &str = r#"#!/usr/bin/env bash
# graphmind UserPromptSubmit hook — always-on context injection (memory + code)

export PATH="$HOME/.graphmind/bin:$HOME/.local/bin:/usr/local/bin:$PATH"

if ! command -v jq &>/dev/null; then exit 0; fi
if ! command -v graphmind &>/dev/null; then exit 0; fi

INPUT=$(cat)
PROMPT=$(echo "$INPUT" | jq -r '.user_prompt // .prompt // empty')

# Skip very short prompts (< 10 chars)
if [ ${#PROMPT} -lt 10 ]; then exit 0; fi

# Check if we're in a graphmind-registered project
graphmind status &>/dev/null || exit 0

# --- Memory checkpoint every 10 turns ---
# Inject a strong save reminder mid-session so long sessions don't lose knowledge.
TURN_COUNT=$(echo "$INPUT" | jq -r '.num_turns // 0')
CHECKPOINT_MSG=""
if [ "$TURN_COUNT" -gt 0 ] && [ $((TURN_COUNT % 10)) -eq 0 ]; then
  CHECKPOINT_MSG="[graphmind checkpoint — turn $TURN_COUNT] Review the last 10 turns and call gm_memory_add NOW for any decisions, conventions, or non-obvious facts that emerged. Be specific and atomic. Use --type decision|pattern|convention|bug. Use --priority for facts needed every session. Do this before continuing."
fi

# --- Priority memories: ALWAYS injected regardless of prompt content ---
PRIORITY_MEM=$(graphmind memory list --priority --limit 10 2>/dev/null | grep -v "^>>" | grep -v "^$" | head -15)

# --- Extract keywords for contextual memory + code search ---
SEARCH_TERMS=$(echo "$PROMPT" \
  | tr '[:upper:]' '[:lower:]' \
  | sed -E 's/[^a-z0-9_]+/ /g' \
  | tr ' ' '\n' \
  | grep -vE '^(comment|how|where|what|who|show|montre|explique|explain|trace|parcour|fonctionne|marche|does|is|me|moi|la|le|les|the|this|that|a|an|de|du|des|un|une|et|ou|en|au|pour|avec|sur|dans|qui|que|ce|ca|il|elle|nous|vous|ils|sont|est|a|ont|fait|faire|peut|doit|from|to|in|of|and|or|but|not|for|with|on|at|by|all|each|can|will|would|should|could|may|might|just|also|very|too|here|there|now|then|be|been|have|has|had|do|it|its|i|you|we|they|je|tu|il|on|ne|pas|plus|aussi|bien|tout|rien|encore|deja|toujours|jamais|trop|peu|maintenant|ici|oui|non|ok|go|fais|veux|peux|dois|bump|version|commit|push|test|build|run|check|fix|add|remove|update|change|make|create|delete|set|get|put)$' \
  | head -5 \
  | tr '\n' ' ' \
  | sed 's/ *$//')

# Cache deduplication: skip contextual search if same terms queried in last 3 minutes
CACHE_FILE="/tmp/graphmind-hook-cache.txt"
NOW=$(date +%s)

MEMORY=""
CODE_RESULTS=""

if [ -n "$SEARCH_TERMS" ]; then
  NORM=$(echo "$SEARCH_TERMS" | tr '[:upper:]' '[:lower:]' | tr -s ' ')
  CACHED=0
  if [ -f "$CACHE_FILE" ]; then
    while IFS='|' read -r TS Q; do
      if [ "$Q" = "$NORM" ] && [ $((NOW - TS)) -lt 180 ]; then
        CACHED=1
        break
      fi
    done < <(tail -50 "$CACHE_FILE")
  fi
  if [ "$CACHED" -eq 0 ]; then
    # Memory recall — always search for relevant context (not just exploration)
    MEMORY=$(graphmind memory search "$SEARCH_TERMS" --limit 5 2>/dev/null | grep -v "^>>" | grep -v "^$" | grep -v "^!" | head -15)

    # Code pre-fetch — for exploration OR implementation prompts mentioning code entities
    if echo "$PROMPT" | grep -qiE 'comment (fonctionne|marche)|how does|where is|où est|show me|montre|architecture|structure|schema|diagram|explique|explain|what does|qui appelle|who calls|trace|flow|parcour|find|cherche|search|look|refactor|implement|ajoute|add.*to|modify|change.*in|fix.*in|debug|implémente|créer|create'; then
      CODE_RESULTS=$(graphmind search "$SEARCH_TERMS" --limit 5 2>/dev/null | head -30)
    fi

    echo "${NOW}|${NORM}" >> "$CACHE_FILE"
    tail -50 "$CACHE_FILE" > "$CACHE_FILE.tmp" 2>/dev/null && mv "$CACHE_FILE.tmp" "$CACHE_FILE" 2>/dev/null
  fi
fi

# Build context message — always provide something useful
MSG=""

if [ -n "$CHECKPOINT_MSG" ]; then
  MSG="$CHECKPOINT_MSG\n\n"
fi

if [ -n "$PRIORITY_MEM" ]; then
  MSG="${MSG}[graphmind ★ active context]\n$PRIORITY_MEM\n\n"
fi

if [ -n "$MEMORY" ]; then
  MSG="${MSG}[graphmind memory] Relevant:\n$MEMORY\n\n"
fi

if [ -n "$CODE_RESULTS" ]; then
  MSG="${MSG}[graphmind code] Relevant symbols:\n$CODE_RESULTS\n\n"
fi

# If nothing found, still confirm graphmind is active
if [ -z "$MSG" ]; then
  MSG="[graphmind] Active. Memory: gm_memory_add (--priority for always-on). Code: gm_search, gm_fn, gm_deps."
fi

jq -n --arg msg "$MSG" '{
  "hookSpecificOutput": {
    "hookEventName": "UserPromptSubmit",
    "additionalContext": $msg
  }
}'
"#;

const STOP_HOOK: &str = r#"#!/usr/bin/env bash
# graphmind Stop hook — auto-consolidate memory + remind Claude to save knowledge

export PATH="$HOME/.graphmind/bin:$HOME/.local/bin:/usr/local/bin:$PATH"

if ! command -v jq &>/dev/null; then exit 0; fi
if ! command -v graphmind &>/dev/null; then exit 0; fi

INPUT=$(cat)

# Skip if no meaningful session (< 3 turns)
TURN_COUNT=$(echo "$INPUT" | jq -r '.num_turns // 0')
if [ "$TURN_COUNT" -lt 3 ]; then exit 0; fi

# Run memory consolidation in background (expire, dedup, auto-promote) — local only, no external API
graphmind memory consolidate &>/dev/null &

# Detect if we're in a registered project
IN_PROJECT=false
graphmind status &>/dev/null && IN_PROJECT=true

if [ "$IN_PROJECT" = true ]; then
  SCOPE="Save project-specific facts without extra flags. For cross-project or user-level facts, add --global."
else
  SCOPE="Not in a registered project — use --global for facts that apply across sessions and projects."
fi

MSG="Session ended ($TURN_COUNT turns). Review this session and call gm_memory_add for any of these categories:

1. ARCHITECTURAL DECISIONS — why something was built a certain way
   Example: \"RRF fusion chosen over pure semantic search because FTS5 handles exact symbol names better\"

2. NEGATIVE DECISIONS — what was tried and rejected, and why
   Example: \"DefaultHasher rejected for device fingerprint — non-deterministic across Rust versions, use SHA256\"

3. INTER-MODULE CONTRACTS — implicit interfaces the code does not make obvious
   Example: \"Global memory lives in global.jsonl, project memory in <slug>.jsonl — never mixed, list() merges both\"

4. CONVENTIONS — naming, patterns, file structure rules
   Example: \"All MCP handlers follow handle_<tool_name>(args: &Value) -> Value signature\"

5. NON-OBVIOUS BUGS & ROOT CAUSES — subtle failures and their fix
   Example: \"cargo test --doc cannot be mixed with --lib --bins — drop --doc flag\"

6. CRITICAL CONSTRAINTS — environment, API keys, external dependencies
   Example: \"Embedding disabled silently if no VOYAGE_API_KEY — check config.embedding.mode first\"

DO NOT save: task summaries, obvious code facts, temporary state, git commit messages.
Use --priority for facts needed at every session start. Be atomic: one fact per entry.

$SCOPE"

jq -n --arg msg "$MSG" '{
  "hookSpecificOutput": {
    "hookEventName": "Stop",
    "additionalContext": $msg
  }
}'
"#;

const POST_TOOL_HOOK: &str = r#"#!/usr/bin/env bash
# graphmind PostToolUse hook — tracks hot files + nudges toward MCP tools

export PATH="$HOME/.graphmind/bin:$HOME/.local/bin:/usr/local/bin:$PATH"

if ! command -v jq &>/dev/null; then exit 0; fi
if ! command -v graphmind &>/dev/null; then exit 0; fi

INPUT=$(cat)
TOOL_NAME=$(echo "$INPUT" | jq -r '.tool_name // .toolName // empty')

# Check if we're in a graphmind-registered project
graphmind status &>/dev/null || exit 0

# --- Track file activity (Read/Edit/Write/Bash) for hot-path detection ---
HOT_FILE="/tmp/graphmind-hot-files.txt"
NOW=$(date +%s)

track_file() {
  local FILE="$1"
  if [ -n "$FILE" ] && [ "$FILE" != "null" ] && echo "$FILE" | grep -qE '\.(rs|ts|tsx|py|go|rb|js|jsx|sql|toml|json)$'; then
    echo "${NOW}|${FILE}" >> "$HOT_FILE"
    tail -100 "$HOT_FILE" > "$HOT_FILE.tmp" 2>/dev/null && mv "$HOT_FILE.tmp" "$HOT_FILE" 2>/dev/null
  fi
}

EDIT_MEMORY_STAMP="/tmp/graphmind-edit-memory.stamp"

case "$TOOL_NAME" in
  Read|Edit|Write)
    FILE=$(echo "$INPUT" | jq -r '.tool_input.file_path // .tool_input.path // empty')
    track_file "$FILE"

    # After Edit/Write on a source file, remind Claude to save any decision that drove the change.
    # Throttle: at most once every 5 minutes to avoid noise.
    if [ "$TOOL_NAME" = "Edit" ] || [ "$TOOL_NAME" = "Write" ]; then
      if echo "$FILE" | grep -qE '\.(rs|ts|tsx|py|go|rb|js|jsx)$'; then
        LAST_EDIT_STAMP=0
        [ -f "$EDIT_MEMORY_STAMP" ] && LAST_EDIT_STAMP=$(cat "$EDIT_MEMORY_STAMP" 2>/dev/null || echo 0)
        if [ $((NOW - LAST_EDIT_STAMP)) -gt 300 ]; then
          echo "$NOW" > "$EDIT_MEMORY_STAMP"
          MSG="[graphmind] You just modified $FILE. If an architectural decision, convention, or non-obvious constraint drove this change, save it now via gm_memory_add before continuing."
          jq -n --arg msg "$MSG" '{
            "hookSpecificOutput": {
              "hookEventName": "PostToolUse",
              "additionalContext": $msg
            }
          }'
          exit 0
        fi
      fi
    fi
    ;;
  Bash)
    CMD=$(echo "$INPUT" | jq -r '.tool_input.command // empty')
    FILES=$(echo "$CMD" | grep -oE '[a-zA-Z0-9_/.-]+\.(rs|ts|tsx|py|go|rb|js|jsx)' | head -3)
    for F in $FILES; do track_file "$F"; done
    ;;
esac

# --- Nudge toward graphmind for broad searches ---
case "$TOOL_NAME" in
  Grep|Bash|Glob|LS) ;;
  *) exit 0 ;;
esac

OUTPUT_LINES=$(echo "$INPUT" | jq -r '.tool_output // .output // empty' | wc -l)
if [ "$OUTPUT_LINES" -lt 20 ]; then
  exit 0
fi

MSG="Note: ${OUTPUT_LINES}+ lines returned. graphmind MCP tools (gm_search, gm_fn, gm_deps) are faster for code navigation."

jq -n --arg msg "$MSG" '{
  "hookSpecificOutput": {
    "hookEventName": "PostToolUse",
    "additionalContext": $msg
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

fn session_hook_path() -> PathBuf {
    hooks_dir().join("graphmind-session.sh")
}

fn prompt_hook_path() -> PathBuf {
    hooks_dir().join("graphmind-prompt.sh")
}

fn post_hook_path() -> PathBuf {
    hooks_dir().join("graphmind-post.sh")
}

fn stop_hook_path() -> PathBuf {
    hooks_dir().join("graphmind-stop.sh")
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

    // Write all hook scripts
    let hook_p = hook_path();
    let session_p = session_hook_path();
    let prompt_p = prompt_hook_path();
    let post_p = post_hook_path();

    let stop_p = stop_hook_path();

    let scripts = [
        (&hook_p, PRE_TOOL_HOOK, "PreToolUse"),
        (&session_p, SESSION_START_HOOK, "SessionStart"),
        (&prompt_p, USER_PROMPT_HOOK, "UserPromptSubmit"),
        (&post_p, POST_TOOL_HOOK, "PostToolUse"),
        (&stop_p, STOP_HOOK, "Stop"),
    ];

    for (path, content, name) in &scripts {
        // Backup existing hook before overwriting
        if path.exists() {
            let ts = chrono::Utc::now().format("%Y%m%d_%H%M%S");
            let backup = path.with_extension(format!("sh.{ts}.bak"));
            fs::copy(path, &backup).ok();
        }
        fs::write(path, content).unwrap_or_else(|e| {
            eprintln!("{} Failed to write {} hook: {}", "Error:".red().bold(), name, e);
            std::process::exit(1);
        });

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(*path, fs::Permissions::from_mode(0o755)).ok();
        }

        println!("{} {} hook → {}", "OK".green().bold(), name, path.display());
    }

    // Register hooks in Claude Code settings.json
    if let Err(e) = register_in_settings() {
        eprintln!("{} Hooks installed but failed to register in settings: {}", "Warning:".yellow().bold(), e);
        println!("  Manually add hooks to ~/.claude/settings.json");
    } else {
        println!("{} All hooks registered in Claude Code settings", "OK".green().bold());
    }
}

pub fn uninstall_hook() {
    let paths = [hook_path(), session_hook_path(), prompt_hook_path(), post_hook_path(), stop_hook_path()];

    for path in &paths {
        if path.exists() {
            fs::remove_file(path).ok();
        }
    }

    if let Err(e) = unregister_from_settings() {
        eprintln!("{} Failed to unregister from settings: {}", "Warning:".yellow().bold(), e);
    } else {
        println!("{} All hooks unregistered from Claude Code settings", "OK".green().bold());
    }

    println!("{} Hooks uninstalled", "OK".green().bold());
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

    let hooks_obj = hooks.as_object_mut().ok_or("hooks is not an object")?;

    let hook_cmd = hook_path().to_string_lossy().to_string();
    let session_cmd = session_hook_path().to_string_lossy().to_string();
    let prompt_cmd = prompt_hook_path().to_string_lossy().to_string();
    let post_cmd = post_hook_path().to_string_lossy().to_string();

    // --- PreToolUse ---
    let pre_tool_use = hooks_obj
        .entry("PreToolUse")
        .or_insert_with(|| serde_json::json!([]));

    let arr = pre_tool_use.as_array_mut().ok_or("PreToolUse is not an array")?;

    let pre_matchers = ["Grep", "Glob", "LS", "Agent"];
    for matcher_name in &pre_matchers {
        let has_graphmind = arr.iter().any(|entry| {
            entry.get("matcher").and_then(|m| m.as_str()) == Some(matcher_name)
                && entry.get("hooks").and_then(|h| h.as_array()).is_some_and(|hooks| {
                    hooks.iter().any(|h| {
                        h.get("command").and_then(|c| c.as_str()).is_some_and(|c| c.contains("graphmind"))
                    })
                })
        });

        if !has_graphmind {
            arr.push(serde_json::json!({
                "matcher": matcher_name,
                "hooks": [{"type": "command", "command": &hook_cmd}]
            }));
        }
    }

    // Bash — merge with existing
    let has_graphmind_bash = arr.iter().any(|entry| {
        entry.get("matcher").and_then(|m| m.as_str()) == Some("Bash")
            && entry.get("hooks").and_then(|h| h.as_array()).is_some_and(|hooks| {
                hooks.iter().any(|h| {
                    h.get("command").and_then(|c| c.as_str()).is_some_and(|c| c.contains("graphmind"))
                })
            })
    });

    if !has_graphmind_bash {
        let existing_bash = arr.iter_mut().find(|entry| {
            entry.get("matcher").and_then(|m| m.as_str()) == Some("Bash")
        });

        if let Some(bash_matcher) = existing_bash {
            if let Some(hooks_arr) = bash_matcher.get_mut("hooks").and_then(|h| h.as_array_mut()) {
                hooks_arr.push(serde_json::json!({"type": "command", "command": &hook_cmd}));
            }
        } else {
            arr.push(serde_json::json!({
                "matcher": "Bash",
                "hooks": [{"type": "command", "command": &hook_cmd}]
            }));
        }
    }

    // --- SessionStart ---
    let session_start = hooks_obj
        .entry("SessionStart")
        .or_insert_with(|| serde_json::json!([]));
    let session_arr = session_start.as_array_mut().ok_or("SessionStart is not an array")?;

    let has_graphmind_session = session_arr.iter().any(|entry| {
        entry.get("hooks").and_then(|h| h.as_array()).is_some_and(|hooks| {
            hooks.iter().any(|h| {
                h.get("command").and_then(|c| c.as_str()).is_some_and(|c| c.contains("graphmind"))
            })
        })
    });

    if !has_graphmind_session {
        session_arr.push(serde_json::json!({
            "matcher": "*",
            "hooks": [{"type": "command", "command": &session_cmd}]
        }));
    }

    // --- UserPromptSubmit ---
    let user_prompt = hooks_obj
        .entry("UserPromptSubmit")
        .or_insert_with(|| serde_json::json!([]));
    let prompt_arr = user_prompt.as_array_mut().ok_or("UserPromptSubmit is not an array")?;

    let has_graphmind_prompt = prompt_arr.iter().any(|entry| {
        entry.get("hooks").and_then(|h| h.as_array()).is_some_and(|hooks| {
            hooks.iter().any(|h| {
                h.get("command").and_then(|c| c.as_str()).is_some_and(|c| c.contains("graphmind"))
            })
        })
    });

    if !has_graphmind_prompt {
        prompt_arr.push(serde_json::json!({
            "matcher": "*",
            "hooks": [{"type": "command", "command": &prompt_cmd}]
        }));
    }

    // --- PostToolUse ---
    let post_tool = hooks_obj
        .entry("PostToolUse")
        .or_insert_with(|| serde_json::json!([]));
    let post_arr = post_tool.as_array_mut().ok_or("PostToolUse is not an array")?;

    let has_graphmind_post = post_arr.iter().any(|entry| {
        entry.get("hooks").and_then(|h| h.as_array()).is_some_and(|hooks| {
            hooks.iter().any(|h| {
                h.get("command").and_then(|c| c.as_str()).is_some_and(|c| c.contains("graphmind"))
            })
        })
    });

    if !has_graphmind_post {
        let post_matchers = ["Grep", "Bash", "Glob", "LS"];
        for matcher_name in &post_matchers {
            post_arr.push(serde_json::json!({
                "matcher": matcher_name,
                "hooks": [{"type": "command", "command": &post_cmd}]
            }));
        }
    }

    // --- Stop ---
    let stop_cmd = stop_hook_path().to_string_lossy().to_string();
    let stop = hooks_obj
        .entry("Stop")
        .or_insert_with(|| serde_json::json!([]));
    let stop_arr = stop.as_array_mut().ok_or("Stop is not an array")?;

    let has_graphmind_stop = stop_arr.iter().any(|entry| {
        entry.get("hooks").and_then(|h| h.as_array()).is_some_and(|hooks| {
            hooks.iter().any(|h| {
                h.get("command").and_then(|c| c.as_str()).is_some_and(|c| c.contains("graphmind"))
            })
        })
    });

    if !has_graphmind_stop {
        stop_arr.push(serde_json::json!({
            "matcher": "*",
            "hooks": [{"type": "command", "command": &stop_cmd}]
        }));
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
        let events = ["PreToolUse", "SessionStart", "UserPromptSubmit", "PostToolUse", "Stop"];

        for event in &events {
            if let Some(arr) = hooks.get_mut(*event).and_then(|p| p.as_array_mut()) {
                // Remove entries that only have graphmind hooks
                arr.retain(|entry| {
                    let hooks_arr = entry.get("hooks").and_then(|h| h.as_array());
                    if let Some(hooks) = hooks_arr {
                        let all_graphmind = hooks.iter().all(|h| {
                            h.get("command").and_then(|c| c.as_str()).is_some_and(|c| c.contains("graphmind"))
                        });
                        if all_graphmind {
                            return false;
                        }
                    }
                    true
                });

                // For entries with mixed hooks, remove only graphmind ones
                for entry in arr.iter_mut() {
                    if let Some(hooks_arr) = entry.get_mut("hooks").and_then(|h| h.as_array_mut()) {
                        hooks_arr.retain(|h| {
                            !h.get("command").and_then(|c| c.as_str()).is_some_and(|c| c.contains("graphmind"))
                        });
                    }
                }

                // Remove entries with empty hooks arrays
                arr.retain(|entry| {
                    entry.get("hooks").and_then(|h| h.as_array()).is_none_or(|hooks| !hooks.is_empty())
                });
            }
        }
    }

    let output = serde_json::to_string_pretty(&settings)
        .map_err(|e| format!("Failed to serialize settings: {}", e))?;

    fs::write(&path, output)
        .map_err(|e| format!("Failed to write settings.json: {}", e))?;

    Ok(())
}

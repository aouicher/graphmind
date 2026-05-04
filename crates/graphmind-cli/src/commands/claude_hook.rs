use colored::Colorize;
use std::fs;
use std::path::PathBuf;

const PRE_TOOL_HOOK: &str = r#"#!/usr/bin/env bash
# graphmind PreToolUse hook — rewrites grep/find to graphmind search (like rtk)
# For Bash: rewrites command. For Grep/Glob/LS/Agent: provides results and skips.

if ! command -v jq &>/dev/null; then exit 0; fi
if ! command -v graphmind &>/dev/null; then exit 0; fi

INPUT=$(cat)
TOOL_NAME=$(echo "$INPUT" | jq -r '.tool_name // .toolName // empty')

# Check if we're in a graphmind-registered project
graphmind status &>/dev/null || exit 0

# Extract search pattern from the tool call
extract_pattern() {
  case "$TOOL_NAME" in
    Grep)
      echo "$INPUT" | jq -r '.tool_input.pattern // .tool_input.regex // empty'
      ;;
    Bash)
      CMD=$(echo "$INPUT" | jq -r '.tool_input.command // empty')
      # Extract pattern from grep/rg/ag commands (may be prefixed with rtk)
      if echo "$CMD" | grep -qE '^\s*(rtk\s+)?(grep|rg|ag)\b'; then
        # Try quoted pattern first, then unquoted
        PAT=$(echo "$CMD" | grep -oE '"[^"]+"' | head -1 | tr -d '"')
        if [ -z "$PAT" ]; then
          PAT=$(echo "$CMD" | grep -oE "'[^']+'" | head -1 | tr -d "'")
        fi
        if [ -z "$PAT" ]; then
          # Last word before path/flags that looks like a pattern
          PAT=$(echo "$CMD" | sed -E 's/^[[:space:]]*(rtk[[:space:]]+)?(grep|rg|ag)[[:space:]]+(-[a-zA-Z]+[[:space:]]+)*//' | awk '{print $1}')
        fi
        echo "$PAT"
      elif echo "$CMD" | grep -qE '^\s*(rtk\s+)?(sed|awk)\b'; then
        # Extract pattern from sed -n '/pattern/p' or awk '/pattern/'
        PAT=$(echo "$CMD" | grep -oE '/[^/]+/' | head -1 | tr -d '/')
        echo "$PAT"
      elif echo "$CMD" | grep -qE '^\s*(rtk\s+)?(find|fd)\b'; then
        # Extract -name pattern or fd pattern
        PAT=$(echo "$CMD" | grep -oE '\-name[[:space:]]+"?[^"]+' | sed 's/-name[[:space:]]*//' | tr -d '"')
        if [ -z "$PAT" ]; then
          PAT=$(echo "$CMD" | sed -E 's/^[[:space:]]*(find|fd)[[:space:]]+[^[:space:]]+[[:space:]]*//' | awk '{print $1}')
        fi
        echo "$PAT"
      fi
      ;;
    Glob)
      echo "$INPUT" | jq -r '.tool_input.pattern // empty' | sed -E 's/.*\///' | sed 's/\*//g'
      ;;
    LS)
      # For LS, use the directory name as context
      echo "$INPUT" | jq -r '.tool_input.path // empty' | sed 's|.*/||'
      ;;
    Agent)
      echo "$INPUT" | jq -r '.tool_input.prompt // empty' \
        | tr '[:upper:]' '[:lower:]' \
        | sed -E 's/[^a-z0-9_]+/ /g' \
        | tr ' ' '\n' \
        | grep -vE '^(the|a|an|is|are|was|were|be|been|being|have|has|had|do|does|did|will|would|could|should|may|might|shall|can|need|must|if|then|else|when|where|which|what|how|who|why|that|this|these|those|it|its|i|you|we|they|he|she|my|your|our|their|his|her|and|or|but|not|no|so|as|at|by|for|from|in|of|on|to|with|about|into|through|during|before|after|above|below|between|under|over|up|down|out|off|all|each|every|both|few|more|most|some|any|other|such|only|just|also|very|too|quite|rather|already|still|yet|even|much|well|here|there|now|then|again|once|always|never|often|sometimes|usually|find|search|explore|look|check|show|get|make|use|see|go|come|take|give|tell|say|know|think|want|try|ask|work|call|run|read|write|set|put|let|keep|start|turn|help|talk|begin|seem|leave|play|move|live|believe|hold|bring|happen|provide|include|continue|change|watch|follow|stop|create|speak|allow|add|grow|open|walk|win|offer|remember|consider|appear|buy|wait|serve|die|send|expect|build|stay|fall|cut|reach|kill|remain|file|files|code|source|project|codebase|repository|repo|directory|function|functions|class|module|component)$' \
        | head -5 \
        | tr '\n' ' ' \
        | sed 's/ *$//'
      ;;
  esac
}

should_intercept() {
  case "$TOOL_NAME" in
    Grep) return 0 ;;
    Bash)
      CMD=$(echo "$INPUT" | jq -r '.tool_input.command // empty')
      echo "$CMD" | grep -qE '^\s*(rtk\s+)?(grep|rg|ag|find|fd|sed|awk)\b' && return 0
      ;;
    Glob|LS) return 0 ;;
    Agent)
      SUBAGENT=$(echo "$INPUT" | jq -r '.tool_input.subagent_type // empty')
      [ "$SUBAGENT" = "Explore" ] && return 0
      PROMPT=$(echo "$INPUT" | jq -r '.tool_input.prompt // empty')
      echo "$PROMPT" | grep -qiE 'search|find|explore|locate|where is|which file|architecture|structure' && return 0
      ;;
  esac
  return 1
}

if ! should_intercept; then exit 0; fi

# Skip rewriting for exhaustive searches — grep is better for "find all occurrences"
# Detect via: description field, recursive grep with -l (file listing), or wc/count patterns
CMD_CHECK=$(echo "$INPUT" | jq -r '(.tool_input.command // "") + " " + (.tool_input.description // "")')
if echo "$CMD_CHECK" | grep -qiE 'partout|tous les|toutes les|every|everywhere|all (usages|occurrences|references|places|files|instances)|exhaustive|each (usage|occurrence|reference|place|instance)|list all|find all|chercher partout|chaque|l.ensemble|la totalit|tout le|complete list|comprehensive|thoroughly'; then
  exit 0
fi
# If grep uses -c (count) or pipes to wc, it's an exhaustive count — let it through
if echo "$CMD_CHECK" | grep -qE '\s-[a-zA-Z]*c|\|\s*wc\s|\|\s*sort\s|\|\s*uniq\s'; then
  exit 0
fi

PATTERN=$(extract_pattern)

# Cache deduplication: skip if same query was searched in last 5 minutes
CACHE_FILE="/tmp/graphmind-hook-cache.txt"
NOW=$(date +%s)
if [ -n "$PATTERN" ] && [ -f "$CACHE_FILE" ]; then
  NORM=$(echo "$PATTERN" | tr '[:upper:]' '[:lower:]' | tr -s ' ')
  while IFS='|' read -r TS Q; do
    if [ "$Q" = "$NORM" ] && [ $((NOW - TS)) -lt 300 ]; then
      exit 0
    fi
  done < <(tail -50 "$CACHE_FILE")
fi

# If no pattern extracted, just provide advice
if [ -z "$PATTERN" ] || [ ${#PATTERN} -gt 200 ]; then
  MSG="⚡ graphmind is indexed. Use MCP gm_search, gm_fn, gm_deps, gm_query instead of grep/find for code patterns."
  jq -n --arg msg "$MSG" '{
    "hookSpecificOutput": {
      "hookEventName": "PreToolUse",
      "additionalContext": $msg
    }
  }'
  exit 0
fi

# For Bash tools: rewrite the command to use graphmind search
if [ "$TOOL_NAME" = "Bash" ]; then
  REWRITTEN="graphmind search \"$PATTERN\" --limit 15"
  ORIGINAL_INPUT=$(echo "$INPUT" | jq -c '.tool_input')
  UPDATED_INPUT=$(echo "$ORIGINAL_INPUT" | jq --arg cmd "$REWRITTEN" '.command = $cmd')

  # Record in cache
  NORM=$(echo "$PATTERN" | tr '[:upper:]' '[:lower:]' | tr -s ' ')
  echo "${NOW}|${NORM}" >> "$CACHE_FILE"
  tail -50 "$CACHE_FILE" > "$CACHE_FILE.tmp" 2>/dev/null && mv "$CACHE_FILE.tmp" "$CACHE_FILE" 2>/dev/null

  jq -n \
    --argjson updated "$UPDATED_INPUT" \
    '{
      "hookSpecificOutput": {
        "hookEventName": "PreToolUse",
        "permissionDecision": "allow",
        "permissionDecisionReason": "graphmind hook: rewritten to graphmind search (code graph indexed)",
        "updatedInput": $updated
      }
    }'
  exit 0
fi

# Execute graphmind search
RESULTS=$(graphmind search "$PATTERN" --limit 15 2>/dev/null | head -60)

# Record in cache
NORM=$(echo "$PATTERN" | tr '[:upper:]' '[:lower:]' | tr -s ' ')
echo "${NOW}|${NORM}" >> "$CACHE_FILE"
tail -50 "$CACHE_FILE" > "$CACHE_FILE.tmp" 2>/dev/null && mv "$CACHE_FILE.tmp" "$CACHE_FILE" 2>/dev/null

if [ -z "$RESULTS" ]; then
  exit 0
fi

# For Agent (sub-agents): BLOCK and return graphmind results — prevents grep/find exploration
if [ "$TOOL_NAME" = "Agent" ]; then
  MSG="⚡ graphmind already has this indexed:\n$RESULTS\n\nUse these results directly. For more detail: graphmind fn <symbol> or graphmind deps <file>."
  jq -n --arg msg "$MSG" '{
    "hookSpecificOutput": {
      "hookEventName": "PreToolUse",
      "permissionDecision": "deny",
      "permissionDecisionReason": $msg
    }
  }'
else
  # For Grep/Glob/LS: inject results as context (don't block — may be legitimate)
  MSG="⚡ graphmind results for \"$PATTERN\" (prefer these over grep/find):\n$RESULTS"
  jq -n --arg msg "$MSG" '{
    "hookSpecificOutput": {
      "hookEventName": "PreToolUse",
      "additionalContext": $msg
    }
  }'
fi
"#;

const SESSION_START_HOOK: &str = r#"#!/usr/bin/env bash
# graphmind SessionStart hook — full project briefing with priority memory

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

if ! command -v jq &>/dev/null; then exit 0; fi
if ! command -v graphmind &>/dev/null; then exit 0; fi

INPUT=$(cat)
PROMPT=$(echo "$INPUT" | jq -r '.user_prompt // .prompt // empty')

# Skip very short prompts (< 10 chars)
if [ ${#PROMPT} -lt 10 ]; then exit 0; fi

# Check if we're in a graphmind-registered project
graphmind status &>/dev/null || exit 0

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

if [ -n "$PRIORITY_MEM" ]; then
  MSG="[graphmind ★ active context]\n$PRIORITY_MEM\n\n"
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

const POST_TOOL_HOOK: &str = r#"#!/usr/bin/env bash
# graphmind PostToolUse hook — tracks hot files + nudges toward MCP tools

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

case "$TOOL_NAME" in
  Read|Edit|Write)
    FILE=$(echo "$INPUT" | jq -r '.tool_input.file_path // .tool_input.path // empty')
    track_file "$FILE"
    ;;
  Bash)
    # Extract file paths from command (common patterns: cargo test FILE, vim FILE, etc.)
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

    let scripts = [
        (&hook_p, PRE_TOOL_HOOK, "PreToolUse"),
        (&session_p, SESSION_START_HOOK, "SessionStart"),
        (&prompt_p, USER_PROMPT_HOOK, "UserPromptSubmit"),
        (&post_p, POST_TOOL_HOOK, "PostToolUse"),
    ];

    for (path, content, name) in &scripts {
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
    let paths = [hook_path(), session_hook_path(), prompt_hook_path(), post_hook_path()];

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
        let events = ["PreToolUse", "SessionStart", "UserPromptSubmit", "PostToolUse"];

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

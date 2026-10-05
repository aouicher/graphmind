use colored::Colorize;
use std::fs;
use std::path::PathBuf;

// Hook scripts. Claude Code replaces hook output above 10,000 characters with a short
// preview, so the context-injecting hooks cut their message to MAX_CHARS (9,000). Hooks run
// without a terminal, so graphmind runs only auto-setup inside them, no notices or
// update/announcement checks (`Background` in main.rs). Every script uses `graphmind which`
// (config only) to decide whether the cwd is a registered project.
const PRE_TOOL_HOOK: &str = r#"#!/usr/bin/env bash
# graphmind PreToolUse hook — lightweight safety net
# Reminds Claude once per session to use graphmind instead of grep/find/Explore agents.
# Does NOT rewrite commands — just nudges via additionalContext.

export PATH="$HOME/.graphmind/bin:$HOME/.local/bin:/usr/local/bin:$PATH"

if ! command -v jq &>/dev/null; then exit 0; fi

INPUT=$(cat)
TOOL_NAME=$(echo "$INPUT" | jq -r '.tool_name // .toolName // empty')

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

# Decide from the input first: most tool calls never need the graphmind binary.
if ! should_nudge; then exit 0; fi

# Skip for exhaustive searches — grep is legitimately better
CMD_CHECK=$(echo "$INPUT" | jq -r '(.tool_input.command // "") + " " + (.tool_input.description // "")')
if echo "$CMD_CHECK" | grep -qiE 'all (usages|occurrences|references|instances)|find all|list all|exhaustive'; then
  exit 0
fi
if echo "$CMD_CHECK" | grep -qE '\s-[a-zA-Z]*c|\|\s*wc\s|\|\s*sort\s|\|\s*uniq\s'; then
  exit 0
fi

# Once per session: repeating the same reminder on every search only costs context.
SESSION_ID=$(echo "$INPUT" | jq -r '.session_id // empty' | tr -cd 'A-Za-z0-9_-')
STATE_DIR="$HOME/.graphmind/hook-state/${SESSION_ID:-default}"
[ -f "$STATE_DIR/nudged" ] && exit 0

if ! command -v graphmind &>/dev/null; then exit 0; fi
graphmind which &>/dev/null || exit 0

mkdir -p "$STATE_DIR" && touch "$STATE_DIR/nudged"

MSG="⚡ This project is graphmind-indexed. Use /gm <query> for code exploration (symbols, callers, deps, outlines). Grep only for string literals/config values."

jq -n --arg msg "$MSG" '{
  "hookSpecificOutput": {
    "hookEventName": "PreToolUse",
    "additionalContext": $msg
  }
}'
"#;

const SESSION_START_HOOK: &str = r#"#!/usr/bin/env bash
# graphmind SessionStart hook — project briefing with priority memory

export PATH="$HOME/.graphmind/bin:$HOME/.local/bin:/usr/local/bin:$PATH"

if ! command -v jq &>/dev/null; then exit 0; fi
if ! command -v graphmind &>/dev/null; then exit 0; fi

INPUT=$(cat)

# Check if we're in a graphmind-registered project
graphmind which &>/dev/null || exit 0

# Per-session state, per user. A new session, /clear or a compaction starts with nothing shown.
STATE_ROOT="$HOME/.graphmind/hook-state"
SESSION_ID=$(echo "$INPUT" | jq -r '.session_id // empty' | tr -cd 'A-Za-z0-9_-')
STATE_DIR="$STATE_ROOT/${SESSION_ID:-default}"
# Drop state of sessions idle for more than 2 days
find "$STATE_ROOT" -mindepth 2 -maxdepth 2 -type f -mtime +2 -delete 2>/dev/null
find "$STATE_ROOT" -mindepth 1 -maxdepth 1 -type d -empty -delete 2>/dev/null
rm -f "$STATE_DIR"/*
mkdir -p "$STATE_DIR"

# Daily memory upkeep (expire, dedup, auto-promote) in the background.
STAMP="$HOME/.graphmind/.last-consolidate"
if [ -z "$(find "$STAMP" -mtime -1 2>/dev/null)" ]; then
  touch "$STAMP"
  (graphmind memory consolidate &>/dev/null &)
fi

MAX_CHARS=9000
NL=$'\n'
# The 8-character id a listing line starts with (after an optional ★). Spelled out for awks
# without regex intervals.
HEX8='[0-9a-f][0-9a-f][0-9a-f][0-9a-f][0-9a-f][0-9a-f][0-9a-f][0-9a-f]'
entry_ids() { awk -v re="$HEX8" 'match($0, re) { print substr($0, RSTART, RLENGTH) }'; }
# Drop lines whose entry id is listed in file $1 (a missing or empty file drops nothing).
drop_shown() {
  awk -v f="$1" -v re="$HEX8" 'BEGIN { while ((getline l < f) > 0) seen[l] = 1 }
    match($0, re) && (substr($0, RSTART, RLENGTH) in seen) { next } { print }'
}

STATUS=$(graphmind status 2>/dev/null | head -10)
MAP=$(graphmind map 2>/dev/null | head -20)

# Priority memories (always-inject: conventions, decisions, preferences), one line each
PRIORITY_MEM=$(graphmind memory list --priority --limit 20 --max-chars 250 2>/dev/null | grep -v "^>>" | grep -v "^$")
# Recent memories that are not already in the priority list
RECENT_MEM=$(graphmind memory list --limit 10 --max-chars 200 2>/dev/null | grep -v "^>>" | grep -v "^$" | grep -v "★" | head -5)

MSG="[graphmind] Project indexed and ready. Use MCP tools (gm_search, gm_fn, gm_deps, gm_map, gm_outline, gm_query) for code exploration.${NL}${NL}Project status:${NL}$STATUS${NL}${NL}Structure:${NL}$MAP"

if [ -n "$PRIORITY_MEM" ]; then
  MSG="$MSG${NL}${NL}★ Priority context (always active):${NL}$PRIORITY_MEM"
fi

if [ -n "$RECENT_MEM" ]; then
  MSG="$MSG${NL}${NL}Recent memories:${NL}$RECENT_MEM"
fi

MSG="$MSG${NL}${NL}Auto-memory is ON: save decisions/patterns/conventions via gm_memory_add. Use --priority for always-injected facts. Recall via gm_memory_search."

if [ ${#MSG} -gt $MAX_CHARS ]; then
  MSG="${MSG:0:$MAX_CHARS}${NL}[graphmind: cut to fit the hook output limit]"
fi

# Remember the priority entries the session sees: whole lines left after the cut, so the
# prompt hook adds the others later.
printf '%s\n' "$MSG" | grep -Fxf <(printf '%s\n' "$PRIORITY_MEM") | entry_ids > "$STATE_DIR/priority-ids"

jq -n --arg msg "$MSG" '{
  "hookSpecificOutput": {
    "hookEventName": "SessionStart",
    "additionalContext": $msg
  }
}'
"#;

const USER_PROMPT_HOOK: &str = r#"#!/usr/bin/env bash
# graphmind UserPromptSubmit hook — relevant memory + code for each prompt

export PATH="$HOME/.graphmind/bin:$HOME/.local/bin:/usr/local/bin:$PATH"

if ! command -v jq &>/dev/null; then exit 0; fi
if ! command -v graphmind &>/dev/null; then exit 0; fi

INPUT=$(cat)
PROMPT=$(echo "$INPUT" | jq -r '.user_prompt // .prompt // empty')

# Skip prompts no person typed: background task results, messages from other sessions,
# slash-command expansions. Their words make poor search terms.
case "$PROMPT" in
  "<task-notification"*|"<cross-session-message"*|"<command-"*|"<local-command"*|"<system-reminder"*) exit 0 ;;
esac

# Skip very short prompts (< 10 chars)
if [ ${#PROMPT} -lt 10 ]; then exit 0; fi

# Check if we're in a graphmind-registered project
graphmind which &>/dev/null || exit 0

MAX_CHARS=9000
NL=$'\n'
# The 8-character id a listing line starts with (after an optional ★). Spelled out for awks
# without regex intervals.
HEX8='[0-9a-f][0-9a-f][0-9a-f][0-9a-f][0-9a-f][0-9a-f][0-9a-f][0-9a-f]'
entry_ids() { awk -v re="$HEX8" 'match($0, re) { print substr($0, RSTART, RLENGTH) }'; }
# Drop lines whose entry id is listed in file $1 (a missing or empty file drops nothing).
drop_shown() {
  awk -v f="$1" -v re="$HEX8" 'BEGIN { while ((getline l < f) > 0) seen[l] = 1 }
    match($0, re) && (substr($0, RSTART, RLENGTH) in seen) { next } { print }'
}
SESSION_ID=$(echo "$INPUT" | jq -r '.session_id // empty' | tr -cd 'A-Za-z0-9_-')
STATE_DIR="$HOME/.graphmind/hook-state/${SESSION_ID:-default}"
mkdir -p "$STATE_DIR"
SHOWN="$STATE_DIR/priority-ids"

# --- Memory checkpoint every 10 prompts ---
# Inject a strong save reminder mid-session so long sessions don't lose knowledge.
TURN_COUNT=$(( $(cat "$STATE_DIR/turns" 2>/dev/null || echo 0) + 1 ))
echo "$TURN_COUNT" > "$STATE_DIR/turns"
CHECKPOINT_MSG=""
if [ $((TURN_COUNT % 10)) -eq 0 ]; then
  CHECKPOINT_MSG="[graphmind checkpoint — prompt $TURN_COUNT] Review the last 10 prompts. Call gm_session_analyze with an array of facts (decisions, conventions, bugs, constraints) that emerged — it batch-saves and deduplicates in one call. Use priority=true for facts needed every session."
fi

# --- Priority memories: SessionStart already showed them. Add only new ones (by id). ---
PRIORITY_MEM=$(graphmind memory list --priority --limit 10 --max-chars 300 2>/dev/null | grep -v "^>>" | grep -v "^$" | drop_shown "$SHOWN")

# --- Search terms: identifiers first (camelCase, snake_case, dotted), then other words ---
STOPWORDS='^(comment|how|where|what|who|why|which|show|montre|explique|explain|trace|parcour|fonctionne|marche|does|is|me|moi|la|le|les|the|this|that|these|those|a|an|de|du|des|un|une|et|ou|en|au|pour|avec|sur|dans|qui|que|ce|ca|il|elle|nous|vous|ils|sont|est|ont|fait|faire|peut|doit|from|to|in|of|and|or|but|not|for|with|on|at|by|all|each|can|will|would|should|could|may|might|just|also|very|too|here|there|now|then|be|been|have|has|had|do|it|its|i|you|we|they|je|tu|ne|pas|plus|aussi|bien|tout|rien|encore|deja|toujours|jamais|trop|peu|maintenant|ici|oui|non|ok|go|fais|veux|peux|dois|bump|version|commit|push|test|build|run|check|fix|add|remove|update|change|make|create|delete|set|get|put|u|ur|pls|please|need|want|like|use|using|see|know|think|sure|yes|any|some|into|about|after|before|again|still|done|let|lets|did|was|were|are|am|our|your|my|their|them|him|her|his|she|he|one|two|other|more|most|much|many|out|up|down|off|over|only|same|than|if|else|so|because|thing|things|way|good|better|best|new|old)$'
IDENTS=$(echo "$PROMPT" | grep -oE '[A-Za-z][A-Za-z0-9]*[a-z0-9][A-Z][A-Za-z0-9]*|[A-Za-z0-9]+(_[A-Za-z0-9]+)+|[A-Za-z][A-Za-z0-9]*(\.[A-Za-z][A-Za-z0-9]*)+' | tr '[:upper:]' '[:lower:]')
WORDS=$(echo "$PROMPT" \
  | tr '[:upper:]' '[:lower:]' \
  | sed -E 's/[^a-z0-9_]+/ /g' \
  | tr ' ' '\n' \
  | grep -E '^[a-z_][a-z0-9_]{2,}$' \
  | grep -vE "$STOPWORDS")
SEARCH_TERMS=$(printf '%s\n%s\n' "$IDENTS" "$WORDS" \
  | grep -v '^$' \
  | awk '!seen[$0]++' \
  | head -5 \
  | tr '\n' ' ' \
  | sed 's/ *$//')

# Cache deduplication: skip contextual search if same terms queried in last 3 minutes
CACHE_FILE="$STATE_DIR/search-cache.txt"
NOW=$(date +%s)

MEMORY=""
CODE_RESULTS=""

if [ -n "$SEARCH_TERMS" ]; then
  NORM=$(echo "$SEARCH_TERMS" | tr -s ' ')
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
    # Memory recall — leave out entries already shown as priority (matched by id)
    MEMORY=$(graphmind memory search "$SEARCH_TERMS" --limit 5 --max-chars 300 2>/dev/null | grep -v "^>>" | grep -v "^$" | grep -v "^!" | drop_shown "$SHOWN" | head -5)

    # Code pre-fetch — for exploration OR implementation prompts mentioning code entities
    if echo "$PROMPT" | grep -qiE 'comment (fonctionne|marche)|how does|where is|où est|show me|montre|architecture|structure|schema|diagram|explique|explain|what does|qui appelle|who calls|trace|flow|parcour|find|cherche|search|look|refactor|implement|ajoute|add.*to|modify|change.*in|fix.*in|debug|implémente|créer|create'; then
      CODE_RESULTS=$(graphmind search "$SEARCH_TERMS" --limit 5 --code-only 2>/dev/null | head -30)
    fi

    echo "${NOW}|${NORM}" >> "$CACHE_FILE"
    tail -50 "$CACHE_FILE" > "$CACHE_FILE.tmp" 2>/dev/null && mv "$CACHE_FILE.tmp" "$CACHE_FILE" 2>/dev/null
  fi
fi

# Build the context message: most specific first, cut to fit the hook output limit.
MSG=""
add_block() {
  [ -z "$2" ] && return
  local block="$1${NL}$2${NL}${NL}"
  local room=$((MAX_CHARS - ${#MSG}))
  if [ ${#block} -le $room ]; then
    MSG="$MSG$block"
  elif [ $room -gt 300 ]; then
    MSG="$MSG${block:0:$((room - 60))}${NL}[graphmind: cut to fit the hook output limit]${NL}"
  fi
}

[ -n "$CHECKPOINT_MSG" ] && MSG="$CHECKPOINT_MSG${NL}${NL}"
add_block "[graphmind memory] Relevant:" "$MEMORY"
add_block "[graphmind code] Relevant symbols:" "$CODE_RESULTS"
add_block "[graphmind ★ new priority context]" "$PRIORITY_MEM"

# Record only the priority entries that made it into the message whole; a cut or dropped one
# is offered again on a later prompt.
[ -n "$PRIORITY_MEM" ] && printf '%s\n' "$MSG" | grep -Fxf <(printf '%s\n' "$PRIORITY_MEM") | entry_ids >> "$SHOWN"

if [ -z "$MSG" ]; then exit 0; fi

jq -n --arg msg "$MSG" '{
  "hookSpecificOutput": {
    "hookEventName": "UserPromptSubmit",
    "additionalContext": $msg
  }
}'
"#;

/// A hook script graphmind installs in `~/.claude/hooks` and registers in settings.json.
struct Hook {
    file: &'static str,
    script: &'static str,
    event: &'static str,
    /// Seconds Claude Code waits for the script.
    timeout: u64,
    matchers: &'static [&'static str],
}

/// The hooks this binary installs. Install, registration, uninstall, detection, the auto-setup
/// fingerprint and the outdated check all loop over this list.
const HOOKS: &[Hook] = &[
    // PreToolUse: only the tools the script can nudge for.
    Hook { file: "graphmind-search.sh", script: PRE_TOOL_HOOK, event: "PreToolUse", timeout: 10, matchers: &["Grep", "Agent", "Bash"] },
    Hook { file: "graphmind-session.sh", script: SESSION_START_HOOK, event: "SessionStart", timeout: 30, matchers: &["*"] },
    Hook { file: "graphmind-prompt.sh", script: USER_PROMPT_HOOK, event: "UserPromptSubmit", timeout: 30, matchers: &["*"] },
];

/// Scripts earlier versions installed: install deletes them, registration drops their entries.
/// PostToolUse (`graphmind-post.sh`) read a field Claude Code doesn't send (`tool_output`) and
/// its matcher never covered Read/Edit/Write; Stop (`graphmind-stop.sh`) read `num_turns`, which
/// Stop input doesn't have. Both exited without doing anything while costing a process per
/// event. Memory upkeep moved to SessionStart.
const RETIRED: &[&str] = &["graphmind-post.sh", "graphmind-stop.sh"];

/// File names of every graphmind hook script, current and retired.
fn all_hook_files() -> impl Iterator<Item = &'static str> {
    HOOKS.iter().map(|h| h.file).chain(RETIRED.iter().copied())
}

fn hooks_dir() -> PathBuf {
    dirs::home_dir()
        .expect("Could not find home directory")
        .join(".claude")
        .join("hooks")
}

fn settings_path() -> PathBuf {
    dirs::home_dir()
        .expect("Could not find home directory")
        .join(".claude")
        .join("settings.json")
}

/// FNV-1a of the hook scripts this binary installs: stable across builds and Rust versions,
/// unlike `DefaultHasher`. Auto-setup's stamp records it, so a new hook version isn't throttled
/// by a stamp left for the old one.
pub fn hooks_fingerprint() -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for hook in HOOKS {
        for byte in hook.script.bytes() {
            hash ^= u64::from(byte);
            hash = hash.wrapping_mul(0x0100_0000_01b3);
        }
    }
    hash
}

/// True when an installed hook script differs from the one this binary would write, so a hook
/// change reaches existing installs through auto-setup without a SETUP_VERSION bump. Scripts
/// that aren't installed don't count: setup never adds hooks nobody asked for.
pub fn installed_hooks_outdated() -> bool {
    let dir = hooks_dir();
    HOOKS
        .iter()
        .any(|hook| fs::read_to_string(dir.join(hook.file)).is_ok_and(|installed| installed != hook.script))
}

pub fn install_hook() {
    let dir = hooks_dir();
    fs::create_dir_all(&dir).unwrap_or_else(|e| {
        eprintln!("{} Failed to create hooks directory: {}", "Error:".red().bold(), e);
        std::process::exit(1);
    });

    for hook in HOOKS {
        let path = dir.join(hook.file);
        // Atomic: auto-setup can run from a hook, and bash reads a running script as it goes,
        // so it must never see a half-rewritten file.
        graphmind_config::write_atomic(&path, hook.script.as_bytes()).unwrap_or_else(|e| {
            eprintln!("{} Failed to write {} hook: {}", "Error:".red().bold(), hook.event, e);
            std::process::exit(1);
        });

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let target = fs::canonicalize(&path).unwrap_or_else(|_| path.clone());
            fs::set_permissions(&target, fs::Permissions::from_mode(0o755)).ok();
        }

        println!("{} {} hook → {}", "OK".green().bold(), hook.event, path.display());
    }

    for retired in RETIRED {
        let path = dir.join(retired);
        if path.exists() {
            fs::remove_file(&path).ok();
        }
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
    let dir = hooks_dir();
    for file in all_hook_files() {
        let path = dir.join(file);
        if path.exists() {
            fs::remove_file(&path).ok();
        }
    }

    if let Err(e) = unregister_from_settings() {
        eprintln!("{} Failed to unregister from settings: {}", "Warning:".yellow().bold(), e);
    } else {
        println!("{} All hooks unregistered from Claude Code settings", "OK".green().bold());
    }

    println!("{} Hooks uninstalled", "OK".green().bold());
}

/// A hook command installed by graphmind (current or retired script).
fn is_graphmind_hook(hook: &serde_json::Value) -> bool {
    hook.get("command")
        .and_then(|c| c.as_str())
        .is_some_and(|c| all_hook_files().any(|file| c.contains(file)))
}

/// Remove graphmind's hook commands from every event, keeping other tools' hooks (including
/// ones that share a matcher entry with ours) and dropping entries and events left empty.
fn strip_graphmind_hooks(hooks_obj: &mut serde_json::Map<String, serde_json::Value>) {
    hooks_obj.retain(|_, list| {
        let Some(arr) = list.as_array_mut() else {
            return true;
        };
        let mut stripped = false;
        for entry in arr.iter_mut() {
            if let Some(hooks_arr) = entry.get_mut("hooks").and_then(|h| h.as_array_mut()) {
                let before = hooks_arr.len();
                hooks_arr.retain(|h| !is_graphmind_hook(h));
                stripped |= hooks_arr.len() != before;
            }
        }
        if !stripped {
            return true;
        }
        arr.retain(|entry| entry.get("hooks").and_then(|h| h.as_array()).is_none_or(|hooks| !hooks.is_empty()));
        !arr.is_empty()
    });
}

fn hook_entry(cmd: &str, timeout_secs: u64) -> serde_json::Value {
    serde_json::json!({"type": "command", "command": cmd, "timeout": timeout_secs})
}

/// Install graphmind's hooks into settings.json. Old graphmind entries are removed first, so
/// re-running setup also migrates earlier installs (retired hooks, unused matchers).
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
    strip_graphmind_hooks(hooks_obj);

    let dir = hooks_dir();
    for hook in HOOKS {
        let cmd = dir.join(hook.file).to_string_lossy().into_owned();
        let list = hooks_obj.entry(hook.event).or_insert_with(|| serde_json::json!([]));
        let arr = list.as_array_mut().ok_or_else(|| format!("{} is not an array", hook.event))?;
        for &matcher in hook.matchers {
            // Join an existing entry for the same matcher, so other tools' hooks there keep
            // running first.
            let existing = arr
                .iter_mut()
                .find(|entry| entry.get("matcher").and_then(|m| m.as_str()) == Some(matcher))
                .and_then(|entry| entry.get_mut("hooks").and_then(|h| h.as_array_mut()));
            match existing {
                Some(hooks_arr) => hooks_arr.push(hook_entry(&cmd, hook.timeout)),
                None => arr.push(serde_json::json!({"matcher": matcher, "hooks": [hook_entry(&cmd, hook.timeout)]})),
            }
        }
    }

    write_settings(&path, &settings)
}

fn unregister_from_settings() -> Result<(), String> {
    let path = settings_path();
    let content = fs::read_to_string(&path)
        .map_err(|e| format!("Failed to read settings.json: {}", e))?;

    let mut settings: serde_json::Value = serde_json::from_str(&content)
        .map_err(|e| format!("Failed to parse settings.json: {}", e))?;

    if let Some(hooks) = settings.get_mut("hooks").and_then(|h| h.as_object_mut()) {
        strip_graphmind_hooks(hooks);
    }

    write_settings(&path, &settings)
}

/// Write settings.json atomically, so a crash or a concurrent writer never leaves Claude Code
/// with a truncated settings file, and a dotfile-managed symlink stays a link.
fn write_settings(path: &std::path::Path, settings: &serde_json::Value) -> Result<(), String> {
    let output = serde_json::to_string_pretty(settings)
        .map_err(|e| format!("Failed to serialize settings: {}", e))?;
    graphmind_config::write_atomic(path, output.as_bytes()).map_err(|e| format!("Failed to write settings.json: {}", e))
}

#[cfg(test)]
mod settings_tests {
    use super::*;
    use serde_json::json;

    /// A dotfile-managed settings.json (a symlink) stays a symlink; its target gets the content.
    #[cfg(unix)]
    #[test]
    fn write_settings_follows_symlinks() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("dotfiles-settings.json");
        let link = dir.path().join("settings.json");
        fs::write(&target, "{}").unwrap();
        std::os::unix::fs::symlink(&target, &link).unwrap();

        write_settings(&link, &json!({"a": 1})).unwrap();

        assert!(fs::symlink_metadata(&link).unwrap().file_type().is_symlink(), "link replaced by a file");
        assert_eq!(serde_json::from_str::<serde_json::Value>(&fs::read_to_string(&target).unwrap()).unwrap(), json!({"a": 1}));
        let entries = fs::read_dir(dir.path()).unwrap().count();
        assert_eq!(entries, 2, "temp file left behind");
    }

    #[test]
    fn strip_keeps_other_tools_hooks() {
        let mut settings = json!({
            "PreToolUse": [
                {"matcher": "Bash", "hooks": [
                    {"type": "command", "command": "/h/guard-bash.py"},
                    {"type": "command", "command": "/h/.claude/hooks/graphmind-search.sh"}
                ]},
                {"matcher": "Glob", "hooks": [{"type": "command", "command": "/h/.claude/hooks/graphmind-search.sh"}]}
            ],
            "PostToolUse": [{"matcher": "Grep", "hooks": [{"type": "command", "command": "/h/.claude/hooks/graphmind-post.sh"}]}],
            "Stop": [{"matcher": "*", "hooks": [{"type": "command", "command": "/h/my-graphmind-notes.sh"}]}]
        });
        strip_graphmind_hooks(settings.as_object_mut().unwrap());
        assert_eq!(
            settings,
            json!({
                "PreToolUse": [{"matcher": "Bash", "hooks": [{"type": "command", "command": "/h/guard-bash.py"}]}],
                "Stop": [{"matcher": "*", "hooks": [{"type": "command", "command": "/h/my-graphmind-notes.sh"}]}]
            })
        );
    }
}

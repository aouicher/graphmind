//! MCP client configs: Claude Desktop, Claude Code (user scope), Cursor, OpenCode and VS Code.
//!
//! One read / compare / back up / write path for all of them, shared by `graphmind setup` and the
//! desktop app. A config that can't be read or parsed, or that has comments a rewrite would drop,
//! is never replaced; an entry that already equals the one graphmind wants is never rewritten (and
//! gets no backup). Nothing here prints: callers report the returned [`Outcome`].

use serde_json::{json, Map, Value};
use std::ffi::OsString;
use std::fs;
use std::io::Read as _;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use crate::paths::{find_on_path, home_dir};

const SERVER_NAME: &str = "graphmind";

/// How long a `claude mcp ...` call may take before graphmind gives up on it and writes the file.
const CLI_TIMEOUT: Duration = Duration::from_secs(30);

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum McpClient {
    ClaudeDesktop,
    /// User scope in ~/.claude.json, where `claude mcp add -s user` puts servers.
    ClaudeCodeUser,
    Cursor,
    OpenCode,
    /// The project folder whose `.vscode/mcp.json` holds the entry.
    VsCode(PathBuf),
}

impl McpClient {
    pub fn label(&self) -> &'static str {
        match self {
            McpClient::ClaudeDesktop => "Claude Desktop",
            McpClient::ClaudeCodeUser => "Claude Code",
            McpClient::Cursor => "Cursor",
            McpClient::OpenCode => "OpenCode",
            McpClient::VsCode(_) => "VS Code",
        }
    }

    pub fn config_path(&self) -> PathBuf {
        match self {
            McpClient::ClaudeDesktop if cfg!(target_os = "windows") => {
                // %APPDATA%\Claude, derived from HOME so tests can override it
                home_dir().join("AppData/Roaming/Claude/claude_desktop_config.json")
            }
            McpClient::ClaudeDesktop if cfg!(target_os = "macos") => {
                home_dir().join("Library/Application Support/Claude/claude_desktop_config.json")
            }
            McpClient::ClaudeDesktop => home_dir().join(".config/claude/claude_desktop_config.json"),
            McpClient::ClaudeCodeUser => {
                claude_code_config_path(&home_dir(), std::env::var_os("CLAUDE_CONFIG_DIR"))
            }
            McpClient::Cursor => home_dir().join(".cursor").join("mcp.json"),
            McpClient::OpenCode => home_dir().join(".config/opencode/opencode.jsonc"),
            McpClient::VsCode(project) => project.join(".vscode").join("mcp.json"),
        }
    }

    /// Whether the client is in use here. Apps create their config folder on first run, so a
    /// missing folder means a missing app: graphmind must not create folders for it.
    pub fn is_installed(&self) -> bool {
        match self {
            // graphmind's hooks and skills target Claude Code; ~/.claude.json lives in HOME itself.
            McpClient::ClaudeCodeUser => true,
            // The OpenCode folder may not exist until its first run, so the CLI counts too.
            McpClient::OpenCode => {
                home_dir().join(".config/opencode").is_dir() || find_on_path("opencode").is_some()
            }
            McpClient::ClaudeDesktop | McpClient::Cursor | McpClient::VsCode(_) => {
                self.config_path().parent().is_some_and(Path::is_dir)
            }
        }
    }

    fn servers_key(&self) -> &'static str {
        match self {
            McpClient::ClaudeDesktop | McpClient::ClaudeCodeUser | McpClient::Cursor => "mcpServers",
            McpClient::OpenCode => "mcp",
            McpClient::VsCode(_) => "servers",
        }
    }

    /// The exact entry this client should hold. No `env` block: `bin` is absolute, and an
    /// `env.PATH` would replace the inherited environment (nvm, asdf, mise...). See issue #105.
    fn entry(&self, bin: &str) -> Value {
        match self {
            McpClient::ClaudeDesktop | McpClient::Cursor => json!({ "command": bin, "args": ["mcp"] }),
            McpClient::ClaudeCodeUser | McpClient::VsCode(_) => {
                json!({ "type": "stdio", "command": bin, "args": ["mcp"] })
            }
            McpClient::OpenCode => json!({ "type": "local", "command": [bin, "mcp"] }),
        }
    }

    /// OpenCode (opencode.jsonc) and VS Code (mcp.json) accept `//` comments.
    fn allows_comments(&self) -> bool {
        matches!(self, McpClient::OpenCode | McpClient::VsCode(_))
    }
}

/// Where Claude Code keeps user-scope servers: `$CLAUDE_CONFIG_DIR/.claude.json` when that is set
/// (`claude mcp add -s user` writes there then), `~/.claude.json` otherwise.
fn claude_code_config_path(home: &Path, config_dir: Option<OsString>) -> PathBuf {
    match config_dir.filter(|dir| !dir.is_empty()) {
        Some(dir) => PathBuf::from(dir).join(".claude.json"),
        None => home.join(".claude.json"),
    }
}

pub enum ConfigRead {
    Missing,
    Parsed { value: Value, has_comments: bool },
    /// Can't be read, isn't valid JSON, or isn't an object. Holds the reason, path included.
    Unparseable(String),
}

/// Read a JSON config that graphmind may update. An empty file reads as `{}`.
pub fn read_json_config(path: &Path, allow_comments: bool) -> ConfigRead {
    let content = match fs::read_to_string(path) {
        Ok(content) => content,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return ConfigRead::Missing,
        Err(e) => return ConfigRead::Unparseable(format!("can't read {} ({e}); left unchanged", path.display())),
    };
    let (text, has_comments) = if allow_comments {
        strip_line_comments(&content)
    } else {
        (content, false)
    };
    if text.trim().is_empty() {
        return ConfigRead::Parsed { value: json!({}), has_comments };
    }
    match serde_json::from_str::<Value>(&text) {
        Ok(value) if value.is_object() => ConfigRead::Parsed { value, has_comments },
        Ok(_) => ConfigRead::Unparseable(format!("{} isn't a JSON object; left unchanged", path.display())),
        Err(e) => ConfigRead::Unparseable(format!("{} is not valid JSON ({e}); left unchanged", path.display())),
    }
}

/// Drop `//` line comments (not inside a string, by a quote-count heuristic). `has_comments` is
/// set only when a line actually lost a comment, not when line endings differ.
fn strip_line_comments(content: &str) -> (String, bool) {
    let mut has_comments = false;
    let lines: Vec<&str> = content
        .lines()
        .map(|line| match line.find("//") {
            Some(idx) if line[..idx].chars().filter(|&c| c == '"').count() % 2 == 0 => {
                has_comments = true;
                line[..idx].trim_end()
            }
            _ => line,
        })
        .collect();
    (lines.join("\n"), has_comments)
}

#[derive(Debug, PartialEq, Eq)]
pub enum Outcome {
    /// Already as wanted: nothing written, no backup.
    Unchanged,
    /// The file was changed (entry added, replaced or removed).
    Written,
    NotInstalled,
    /// The file was not changed, for the reason given (path included).
    LeftAlone(String),
}

impl Outcome {
    /// The entry is in place after this run: written now or already current.
    pub fn is_in_place(&self) -> bool {
        matches!(self, Outcome::Unchanged | Outcome::Written)
    }
}

/// Make `client`'s config hold exactly the entry for `bin`; other keys and servers are kept.
///
/// Claude Code rewrites ~/.claude.json all the time, and an atomic rename prevents a truncated
/// file but not a lost update, so for Claude Code the `claude` CLI does the write when it is on
/// PATH. If the CLI fails, the file is written directly and `on_cli_fallback` gets the reason.
pub fn upsert(client: &McpClient, bin: &str, on_cli_fallback: &mut dyn FnMut(&str)) -> Outcome {
    if !client.is_installed() {
        return Outcome::NotInstalled;
    }
    let wanted = client.entry(bin);
    if *client == McpClient::ClaudeCodeUser {
        if let Some(claude) = find_on_path("claude") {
            if let Some(outcome) = upsert_with_claude_cli(client, &claude, bin, &wanted, on_cli_fallback) {
                return outcome;
            }
        }
    }
    upsert_file(client, &wanted)
}

/// Remove graphmind's entry from `client`'s config, keeping everything else. Claude Code goes
/// through `claude mcp remove -s user` when the CLI is on PATH, like [`upsert`].
pub fn remove(client: &McpClient, on_cli_fallback: &mut dyn FnMut(&str)) -> Outcome {
    if *client == McpClient::ClaudeCodeUser {
        if let Some(claude) = find_on_path("claude") {
            if let Some(outcome) = remove_with_claude_cli(client, &claude, on_cli_fallback) {
                return outcome;
            }
        }
    }
    remove_file(client)
}

/// Whether `client`'s config (readable and parsed) holds a graphmind entry.
pub fn has_entry(client: &McpClient) -> bool {
    matches!(current_entry(client), Ok(Some(_)))
}

/// `upsert` through the `claude` CLI. `None` means the CLI didn't get the entry in place and the
/// file should be written directly.
fn upsert_with_claude_cli(
    client: &McpClient,
    claude: &Path,
    bin: &str,
    wanted: &Value,
    on_cli_fallback: &mut dyn FnMut(&str),
) -> Option<Outcome> {
    let existing = match current_entry(client) {
        Ok(existing) => existing,
        Err(reason) => return Some(Outcome::LeftAlone(reason)),
    };
    if existing.as_ref().is_some_and(|e| is_current(e, wanted)) {
        return Some(Outcome::Unchanged);
    }
    // `mcp add` refuses a name that exists, so a stale entry is removed first.
    let result = match existing {
        Some(_) => run_claude(claude, &["mcp", "remove", "-s", "user", SERVER_NAME]),
        None => Ok(()),
    }
    .and_then(|()| run_claude(claude, &["mcp", "add", "-s", "user", SERVER_NAME, "--", bin, "mcp"]));
    match result {
        Ok(()) if current_entry(client).ok().flatten().is_some_and(|e| is_current(&e, wanted)) => {
            return Some(Outcome::Written);
        }
        Ok(()) => on_cli_fallback(&format!(
            "`claude mcp add` left no matching entry in {}",
            client.config_path().display()
        )),
        Err(reason) => on_cli_fallback(&format!("`claude mcp add` failed ({reason})")),
    }
    None
}

/// `remove` through the `claude` CLI. `None` means the entry is still there and the file should be
/// written directly.
fn remove_with_claude_cli(
    client: &McpClient,
    claude: &Path,
    on_cli_fallback: &mut dyn FnMut(&str),
) -> Option<Outcome> {
    match current_entry(client) {
        Ok(None) => return Some(Outcome::Unchanged),
        Err(reason) => return Some(Outcome::LeftAlone(reason)),
        Ok(Some(_)) => {}
    }
    match run_claude(claude, &["mcp", "remove", "-s", "user", SERVER_NAME]) {
        Ok(()) if matches!(current_entry(client), Ok(None)) => return Some(Outcome::Written),
        Ok(()) => on_cli_fallback(&format!(
            "`claude mcp remove` left the entry in {}",
            client.config_path().display()
        )),
        Err(reason) => on_cli_fallback(&format!("`claude mcp remove` failed ({reason})")),
    }
    None
}

fn upsert_file(client: &McpClient, wanted: &Value) -> Outcome {
    let path = client.config_path();
    let key = client.servers_key();
    let (mut config, has_comments) = match read_json_config(&path, client.allows_comments()) {
        ConfigRead::Missing => (json!({}), false),
        ConfigRead::Parsed { value, has_comments } => (value, has_comments),
        ConfigRead::Unparseable(reason) => return Outcome::LeftAlone(reason),
    };
    if server_entry(&config, key).is_some_and(|e| is_current(e, wanted)) {
        return Outcome::Unchanged;
    }
    if has_comments {
        return Outcome::LeftAlone(format!(
            "{} has comments, which a rewrite would drop; left unchanged. Add this under \"{key}\" yourself:\n      \"{SERVER_NAME}\": {wanted}",
            path.display()
        ));
    }
    let Some(servers) = config
        .as_object_mut()
        .and_then(|root| root.entry(key).or_insert_with(|| json!({})).as_object_mut())
    else {
        return Outcome::LeftAlone(format!("\"{key}\" in {} isn't an object; left unchanged", path.display()));
    };
    servers.insert(SERVER_NAME.to_string(), wanted.clone());
    match write_config(&path, &config) {
        Ok(()) => Outcome::Written,
        Err(reason) => Outcome::LeftAlone(reason),
    }
}

fn remove_file(client: &McpClient) -> Outcome {
    let path = client.config_path();
    let key = client.servers_key();
    let (mut config, has_comments) = match read_json_config(&path, client.allows_comments()) {
        ConfigRead::Missing => return Outcome::Unchanged,
        ConfigRead::Parsed { value, has_comments } => (value, has_comments),
        ConfigRead::Unparseable(reason) => return Outcome::LeftAlone(reason),
    };
    if server_entry(&config, key).is_none() {
        return Outcome::Unchanged;
    }
    if has_comments {
        return Outcome::LeftAlone(format!(
            "{} has comments, which a rewrite would drop; left unchanged. Remove \"{SERVER_NAME}\" under \"{key}\" yourself.",
            path.display()
        ));
    }
    if let Some(servers) = config.get_mut(key).and_then(Value::as_object_mut) {
        servers.remove(SERVER_NAME);
    }
    match write_config(&path, &config) {
        Ok(()) => Outcome::Written,
        Err(reason) => Outcome::LeftAlone(reason),
    }
}

/// Remove the per-project Claude Code entries (`projects.<path>.mcpServers.graphmind` in
/// ~/.claude.json) that older versions wrote, for one project or, with `None`, for all of them.
/// Returns how many were removed.
pub fn remove_claude_project_entries(project: Option<&str>) -> Result<usize, String> {
    let path = McpClient::ClaudeCodeUser.config_path();
    let mut config = match read_json_config(&path, false) {
        ConfigRead::Missing => return Ok(0),
        ConfigRead::Parsed { value, .. } => value,
        ConfigRead::Unparseable(reason) => return Err(reason),
    };
    let mut removed = 0;
    if let Some(projects) = config.get_mut("projects").and_then(Value::as_object_mut) {
        for (key, entry) in projects.iter_mut() {
            if project.is_some_and(|p| p != key) {
                continue;
            }
            if let Some(servers) = entry.get_mut("mcpServers").and_then(Value::as_object_mut) {
                if servers.remove(SERVER_NAME).is_some() {
                    removed += 1;
                }
            }
        }
    }
    if removed > 0 {
        write_config(&path, &config)?;
    }
    Ok(removed)
}

/// Back up `path` (when it exists) and replace it atomically with `config`.
pub fn write_config(path: &Path, config: &Value) -> Result<(), String> {
    let content = serde_json::to_string_pretty(config)
        .map_err(|e| format!("can't serialize {} ({e}); left unchanged", path.display()))?;
    if path.exists() {
        let ts = chrono::Utc::now().format("%Y%m%d_%H%M%S");
        let ext = path.extension().map(|e| e.to_string_lossy().into_owned()).unwrap_or_default();
        let backup = path.with_extension(format!("{ext}.{ts}.bak"));
        fs::copy(path, &backup)
            .map_err(|e| format!("can't back up {} ({e}); left unchanged", path.display()))?;
    } else if let Some(parent) = path.parent() {
        // Reachable for OpenCode found on PATH before its first run created the folder, and for a
        // CLAUDE_CONFIG_DIR that doesn't exist yet.
        fs::create_dir_all(parent).map_err(|e| format!("can't create {} ({e})", parent.display()))?;
    }
    crate::write_atomic(path, content.as_bytes())
        .map_err(|e| format!("failed to write {} ({e}); left unchanged", path.display()))
}

/// The graphmind binary MCP entries point at: the install location (~/.graphmind/bin) when it
/// exists, so the entry doesn't depend on PATH; else the first `graphmind` on PATH; else the
/// install location anyway (an install in progress).
pub fn graphmind_binary() -> String {
    let bin_name = if cfg!(target_os = "windows") { "graphmind.exe" } else { "graphmind" };
    let local_path = home_dir().join(".graphmind").join("bin").join(bin_name);
    if local_path.exists() {
        return local_path.to_string_lossy().to_string();
    }
    if let Some(path) = find_on_path("graphmind") {
        return path.to_string_lossy().to_string();
    }
    local_path.to_string_lossy().to_string()
}

/// The graphmind entry `client`'s config holds now. `Err` when the file can't be read or parsed.
fn current_entry(client: &McpClient) -> Result<Option<Value>, String> {
    match read_json_config(&client.config_path(), client.allows_comments()) {
        ConfigRead::Missing => Ok(None),
        ConfigRead::Parsed { value, .. } => Ok(server_entry(&value, client.servers_key()).cloned()),
        ConfigRead::Unparseable(reason) => Err(reason),
    }
}

fn server_entry<'a>(config: &'a Value, key: &str) -> Option<&'a Value> {
    config.get(key).and_then(|servers| servers.get(SERVER_NAME))
}

/// `existing` equals `wanted`, treating an empty `env` object (which `claude mcp add` writes) as
/// no `env` at all.
fn is_current(existing: &Value, wanted: &Value) -> bool {
    let mut existing = existing.clone();
    if let Some(entry) = existing.as_object_mut() {
        if entry.get("env").and_then(Value::as_object).is_some_and(Map::is_empty) {
            entry.remove("env");
        }
    }
    existing == *wanted
}

/// Run the `claude` CLI, killing it after `CLI_TIMEOUT`. The error is its first stderr line.
fn run_claude(claude: &Path, args: &[&str]) -> Result<(), String> {
    let mut child = Command::new(claude)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| e.to_string())?;
    let deadline = Instant::now() + CLI_TIMEOUT;
    loop {
        match child.try_wait() {
            Ok(Some(status)) if status.success() => return Ok(()),
            Ok(Some(status)) => {
                let mut stderr = String::new();
                if let Some(mut pipe) = child.stderr.take() {
                    pipe.read_to_string(&mut stderr).ok();
                }
                let first_line = stderr.lines().map(str::trim).find(|l| !l.is_empty());
                return Err(first_line.map_or_else(|| status.to_string(), str::to_string));
            }
            Ok(None) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(50)),
            Ok(None) => {
                child.kill().ok();
                child.wait().ok();
                return Err(format!("no answer after {}s", CLI_TIMEOUT.as_secs()));
            }
            Err(e) => return Err(e.to_string()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn no_fallback(reason: &str) {
        panic!("only Claude Code uses the CLI: {reason}");
    }

    fn backups_in(dir: &Path) -> usize {
        fs::read_dir(dir).unwrap().flatten().filter(|e| e.file_name().to_string_lossy().ends_with(".bak")).count()
    }

    #[test]
    fn trailing_newline_and_crlf_are_not_comments() {
        assert_eq!(strip_line_comments("{\"foo\": \"bar\"}\n"), ("{\"foo\": \"bar\"}".to_string(), false));
        assert!(!strip_line_comments("{\r\n  \"url\": \"http://x\"\r\n}\r\n").1, "// inside a string");
        assert!(strip_line_comments("// mine\n{}\n").1);
        assert!(strip_line_comments("{\"a\": 1} // trailing\n").1);
    }

    #[test]
    fn empty_env_counts_as_no_env() {
        let wanted = json!({ "type": "stdio", "command": "/bin/gm", "args": ["mcp"] });
        assert!(is_current(&json!({ "type": "stdio", "command": "/bin/gm", "args": ["mcp"], "env": {} }), &wanted));
        assert!(!is_current(&json!({ "type": "stdio", "command": "/bin/gm", "args": ["mcp"], "env": { "PATH": "/x" } }), &wanted));
        assert!(!is_current(&json!({ "type": "stdio", "command": "/old/gm", "args": ["mcp"] }), &wanted));
    }

    #[test]
    fn claude_code_config_honours_claude_config_dir() {
        let home = Path::new("/home/me");
        assert_eq!(claude_code_config_path(home, None), home.join(".claude.json"));
        assert_eq!(claude_code_config_path(home, Some(OsString::new())), home.join(".claude.json"));
        assert_eq!(
            claude_code_config_path(home, Some(OsString::from("/cfg/claude"))),
            Path::new("/cfg/claude/.claude.json")
        );
    }

    #[test]
    fn upsert_and_remove_keep_other_servers_and_skip_no_ops() {
        let project = tempfile::tempdir().unwrap();
        let client = McpClient::VsCode(project.path().to_path_buf());
        assert_eq!(upsert(&client, "/bin/gm", &mut no_fallback), Outcome::NotInstalled);
        assert!(!project.path().join(".vscode").exists(), "no folder for a project without VS Code");

        let vscode = project.path().join(".vscode");
        fs::create_dir(&vscode).unwrap();
        fs::write(client.config_path(), r#"{"servers": {"other": {"command": "other"}}}"#).unwrap();
        assert_eq!(upsert(&client, "/bin/gm", &mut no_fallback), Outcome::Written);
        assert!(has_entry(&client));
        assert_eq!(upsert(&client, "/bin/gm", &mut no_fallback), Outcome::Unchanged);
        assert_eq!(backups_in(&vscode), 1, "a backup for the write, none for the no-op");

        assert_eq!(remove(&client, &mut no_fallback), Outcome::Written);
        assert_eq!(remove(&client, &mut no_fallback), Outcome::Unchanged);
        assert!(!has_entry(&client));
        let config: Value = serde_json::from_str(&fs::read_to_string(client.config_path()).unwrap()).unwrap();
        assert_eq!(config["servers"]["other"]["command"], "other");
    }

    #[test]
    fn unparseable_or_commented_config_is_left_alone() {
        let project = tempfile::tempdir().unwrap();
        let client = McpClient::VsCode(project.path().to_path_buf());
        fs::create_dir(project.path().join(".vscode")).unwrap();

        fs::write(client.config_path(), "{\"servers\": {},}").unwrap();
        assert!(matches!(upsert(&client, "/bin/gm", &mut no_fallback), Outcome::LeftAlone(_)));
        assert_eq!(fs::read_to_string(client.config_path()).unwrap(), "{\"servers\": {},}");

        let commented = "// mine\n{\"servers\": {\"graphmind\": {\"command\": \"/old/gm\"}}}\n";
        fs::write(client.config_path(), commented).unwrap();
        assert!(has_entry(&client));
        assert!(matches!(upsert(&client, "/bin/gm", &mut no_fallback), Outcome::LeftAlone(_)));
        assert!(matches!(remove(&client, &mut no_fallback), Outcome::LeftAlone(_)));
        assert_eq!(fs::read_to_string(client.config_path()).unwrap(), commented);
    }
}

//! MCP client configs: Claude Desktop, Claude Code (per project), Cursor, OpenCode and VS Code.
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

pub(crate) const SERVER_NAME: &str = "graphmind";

/// How long a `claude mcp ...` call may take before graphmind gives up on it and writes the file.
const CLI_TIMEOUT: Duration = Duration::from_secs(30);

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum McpClient {
    ClaudeDesktop,
    /// User scope in .claude.json (top-level `mcpServers`), where `claude mcp add -s user` puts
    /// servers. graphmind registers Claude Code per project, so it only removes its entry here.
    ClaudeCodeUser,
    /// One entry in .claude.json (`projects.<key>.mcpServers`), where `claude mcp add -s local`
    /// run in that folder puts servers. The path is a Claude Code key, the folder Claude Code
    /// reads the project's servers under (see [`crate::claude_code::claude_code_key`]).
    ClaudeCodeProject(PathBuf),
    Cursor,
    OpenCode,
    /// The project folder whose `.vscode/mcp.json` holds the entry.
    VsCode(PathBuf),
}

impl McpClient {
    pub fn label(&self) -> &'static str {
        match self {
            McpClient::ClaudeDesktop => "Claude Desktop",
            McpClient::ClaudeCodeUser | McpClient::ClaudeCodeProject(_) => "Claude Code",
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
            McpClient::ClaudeCodeUser | McpClient::ClaudeCodeProject(_) => {
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
            McpClient::ClaudeCodeUser | McpClient::ClaudeCodeProject(_) => true,
            // The OpenCode folder may not exist until its first run, so the CLI counts too.
            McpClient::OpenCode => {
                home_dir().join(".config/opencode").is_dir() || find_on_path("opencode").is_some()
            }
            McpClient::ClaudeDesktop | McpClient::Cursor | McpClient::VsCode(_) => {
                self.config_path().parent().is_some_and(Path::is_dir)
            }
        }
    }

    /// The keys leading to the object that holds the client's servers.
    pub(crate) fn servers_path(&self) -> Vec<String> {
        let key = match self {
            McpClient::ClaudeCodeProject(project) => {
                return vec!["projects".to_string(), project.to_string_lossy().into_owned(), "mcpServers".to_string()];
            }
            McpClient::ClaudeDesktop | McpClient::ClaudeCodeUser | McpClient::Cursor => "mcpServers",
            McpClient::OpenCode => "mcp",
            McpClient::VsCode(_) => "servers",
        };
        vec![key.to_string()]
    }

    /// The `claude mcp` scope and working directory, for the clients the `claude` CLI manages.
    /// Run in a Claude Code key, `claude mcp add -s local` files the server under that key, except
    /// in a key inside a `.git` directory (a submodule's worktree reads
    /// `<superproject>/.git/modules/<name>`): run there, it files it under the superproject, so
    /// that key is written directly.
    fn claude_cli_scope(&self) -> Option<(&'static str, Option<&Path>)> {
        match self {
            McpClient::ClaudeCodeUser => Some(("user", None)),
            McpClient::ClaudeCodeProject(key) if !key.components().any(|c| c.as_os_str() == ".git") => {
                Some(("local", Some(key.as_path())))
            }
            _ => None,
        }
    }

    fn is_claude_code(&self) -> bool {
        matches!(self, McpClient::ClaudeCodeUser | McpClient::ClaudeCodeProject(_))
    }

    /// The exact entry this client should hold. No `env` block: `bin` is absolute, and an
    /// `env.PATH` would replace the inherited environment (nvm, asdf, mise...). See issue #105.
    pub(crate) fn entry(&self, bin: &str) -> Value {
        match self {
            McpClient::ClaudeDesktop | McpClient::Cursor => json!({ "command": bin, "args": ["mcp"] }),
            McpClient::ClaudeCodeUser | McpClient::ClaudeCodeProject(_) | McpClient::VsCode(_) => {
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

/// Where Claude Code keeps its MCP servers, user scope and per project: `$CLAUDE_CONFIG_DIR/.claude.json`
/// when that is set (`claude mcp add` writes there then), `~/.claude.json` otherwise.
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
    /// Left as the person set it (a hand-made entry), for the reason given: a deliberate state,
    /// not a failure.
    Kept(String),
}

impl Outcome {
    /// The entry is in place after this run: written now or already current.
    pub fn is_in_place(&self) -> bool {
        matches!(self, Outcome::Unchanged | Outcome::Written)
    }
}

/// Make `client`'s config hold exactly the entry for `bin`; other keys and servers are kept.
///
/// Claude Code rewrites .claude.json all the time, and an atomic rename prevents a truncated file
/// but not a lost update, so for Claude Code the `claude` CLI does the write when it is on PATH.
/// If the CLI fails, the file is written directly and `on_cli_fallback` gets the reason. A Claude
/// Code `graphmind` entry that doesn't run graphmind was set up by hand and is left alone.
pub fn upsert(client: &McpClient, bin: &str, on_cli_fallback: &mut dyn FnMut(&str)) -> Outcome {
    if !client.is_installed() {
        return Outcome::NotInstalled;
    }
    if client.is_claude_code() {
        match current_entry(client) {
            Err(reason) => return Outcome::LeftAlone(reason),
            Ok(Some(entry)) if !points_at_graphmind(&entry) => return Outcome::Kept(hand_made(client)),
            Ok(_) => {}
        }
    }
    let wanted = client.entry(bin);
    if let Some(scope) = client.claude_cli_scope() {
        if let Some(claude) = find_on_path("claude") {
            if let Some(outcome) = upsert_with_claude_cli(client, scope, &claude, bin, &wanted, on_cli_fallback) {
                return outcome;
            }
        }
    }
    upsert_file(client, &wanted)
}

/// Remove graphmind's entry from `client`'s config, keeping everything else. Claude Code goes
/// through `claude mcp remove` when the CLI is on PATH, like [`upsert`], and keeps a `graphmind`
/// entry set up by hand.
pub fn remove(client: &McpClient, on_cli_fallback: &mut dyn FnMut(&str)) -> Outcome {
    remove_with(client, true, on_cli_fallback)
}

/// [`remove`], with a direct write that backs the file up first only when `back_up` is set: a run
/// that already backed the file up passes false, so its one backup holds the file as it was.
pub(crate) fn remove_with(client: &McpClient, back_up: bool, on_cli_fallback: &mut dyn FnMut(&str)) -> Outcome {
    if client.is_claude_code() {
        match current_entry(client) {
            Err(reason) => return Outcome::LeftAlone(reason),
            Ok(None) => return Outcome::Unchanged,
            Ok(Some(entry)) if !points_at_graphmind(&entry) => return Outcome::Kept(hand_made(client)),
            Ok(Some(_)) => {}
        }
    }
    if let Some(scope) = client.claude_cli_scope() {
        if let Some(claude) = find_on_path("claude") {
            if let Some(outcome) = remove_with_claude_cli(client, scope, &claude, on_cli_fallback) {
                return outcome;
            }
        }
    }
    remove_file(client, back_up)
}

/// Whether `client`'s config (readable and parsed) holds a graphmind entry.
pub fn has_entry(client: &McpClient) -> bool {
    matches!(current_entry(client), Ok(Some(_)))
}

/// Whether `client`'s config holds a graphmind entry that runs a graphmind binary, i.e. one
/// graphmind wrote rather than a server of the same name set up by hand.
pub fn has_graphmind_entry(client: &McpClient) -> bool {
    matches!(current_entry(client), Ok(Some(entry)) if points_at_graphmind(&entry))
}

fn hand_made(client: &McpClient) -> String {
    format!("{}: left alone: hand-made graphmind entry (its command isn't a graphmind binary)", describe(client))
}

/// `upsert` through the `claude` CLI. `None` means the CLI didn't get the entry in place and the
/// file should be written directly.
fn upsert_with_claude_cli(
    client: &McpClient,
    (scope, cwd): (&str, Option<&Path>),
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
        Some(_) => run_claude(claude, cwd, &["mcp", "remove", "-s", scope, SERVER_NAME]),
        None => Ok(()),
    }
    .and_then(|()| run_claude(claude, cwd, &["mcp", "add", "-s", scope, SERVER_NAME, "--", bin, "mcp"]));
    match result {
        Ok(()) if current_entry(client).ok().flatten().is_some_and(|e| is_current(&e, wanted)) => {
            return Some(Outcome::Written);
        }
        Ok(()) => on_cli_fallback(&format!("`claude mcp add` left no matching entry in {}", describe(client))),
        Err(reason) => on_cli_fallback(&format!("`claude mcp add` failed ({reason})")),
    }
    None
}

/// `remove` through the `claude` CLI, once [`remove`] found graphmind's entry. `None` means the
/// entry is still there and the file should be written directly.
fn remove_with_claude_cli(
    client: &McpClient,
    (scope, cwd): (&str, Option<&Path>),
    claude: &Path,
    on_cli_fallback: &mut dyn FnMut(&str),
) -> Option<Outcome> {
    match run_claude(claude, cwd, &["mcp", "remove", "-s", scope, SERVER_NAME]) {
        Ok(()) if matches!(current_entry(client), Ok(None)) => return Some(Outcome::Written),
        Ok(()) => on_cli_fallback(&format!("`claude mcp remove` left the entry in {}", describe(client))),
        Err(reason) => on_cli_fallback(&format!("`claude mcp remove` failed ({reason})")),
    }
    None
}

/// The config file, plus the project for a per-project Claude Code entry.
pub fn describe(client: &McpClient) -> String {
    match client {
        McpClient::ClaudeCodeProject(project) => {
            format!("{} (project {})", client.config_path().display(), project.display())
        }
        _ => client.config_path().display().to_string(),
    }
}

fn upsert_file(client: &McpClient, wanted: &Value) -> Outcome {
    let path = client.config_path();
    let keys = client.servers_path();
    let (mut config, has_comments) = match read_json_config(&path, client.allows_comments()) {
        ConfigRead::Missing => (json!({}), false),
        ConfigRead::Parsed { value, has_comments } => (value, has_comments),
        ConfigRead::Unparseable(reason) => return Outcome::LeftAlone(reason),
    };
    if server_entry(&config, &keys).is_some_and(|e| is_current(e, wanted)) {
        return Outcome::Unchanged;
    }
    let key = keys.join(".");
    if has_comments {
        return Outcome::LeftAlone(format!(
            "{} has comments, which a rewrite would drop; left unchanged. Add this under \"{key}\" yourself:\n      \"{SERVER_NAME}\": {wanted}",
            path.display()
        ));
    }
    let Some(servers) = servers_mut(&mut config, &keys) else {
        return Outcome::LeftAlone(format!("\"{key}\" in {} isn't an object; left unchanged", path.display()));
    };
    servers.insert(SERVER_NAME.to_string(), wanted.clone());
    match write_config(&path, &config) {
        Ok(()) => Outcome::Written,
        Err(reason) => Outcome::LeftAlone(reason),
    }
}

fn remove_file(client: &McpClient, back_up: bool) -> Outcome {
    let path = client.config_path();
    let keys = client.servers_path();
    let (mut config, has_comments) = match read_json_config(&path, client.allows_comments()) {
        ConfigRead::Missing => return Outcome::Unchanged,
        ConfigRead::Parsed { value, has_comments } => (value, has_comments),
        ConfigRead::Unparseable(reason) => return Outcome::LeftAlone(reason),
    };
    if server_entry(&config, &keys).is_none() {
        return Outcome::Unchanged;
    }
    if has_comments {
        return Outcome::LeftAlone(format!(
            "{} has comments, which a rewrite would drop; left unchanged. Remove \"{SERVER_NAME}\" under \"{}\" yourself.",
            path.display(),
            keys.join(".")
        ));
    }
    let servers = keys.iter().try_fold(&mut config, |value, key| value.get_mut(key.as_str()));
    if let Some(servers) = servers.and_then(Value::as_object_mut) {
        servers.remove(SERVER_NAME);
    }
    match write_json(&path, &config, back_up) {
        Ok(()) => Outcome::Written,
        Err(reason) => Outcome::LeftAlone(reason),
    }
}

/// Back up `path` (when it exists) and replace it atomically with `config`.
pub fn write_config(path: &Path, config: &Value) -> Result<(), String> {
    write_json(path, config, true)
}

/// Replace `path` atomically with `config`, backing it up first (when it exists) if `back_up_first`.
fn write_json(path: &Path, config: &Value, back_up_first: bool) -> Result<(), String> {
    let content = serde_json::to_string_pretty(config)
        .map_err(|e| format!("can't serialize {} ({e}); left unchanged", path.display()))?;
    if path.exists() {
        if back_up_first {
            crate::back_up(path)?;
        }
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
pub(crate) fn current_entry(client: &McpClient) -> Result<Option<Value>, String> {
    match read_json_config(&client.config_path(), client.allows_comments()) {
        ConfigRead::Missing => Ok(None),
        ConfigRead::Parsed { value, .. } => Ok(server_entry(&value, &client.servers_path()).cloned()),
        ConfigRead::Unparseable(reason) => Err(reason),
    }
}

pub(crate) fn server_entry<'a>(config: &'a Value, keys: &[String]) -> Option<&'a Value> {
    keys.iter().try_fold(config, |value, key| value.get(key.as_str()))?.get(SERVER_NAME)
}

/// The object at `keys`, created along with any missing parent. `None` when a value on the way
/// isn't an object.
pub(crate) fn servers_mut<'a>(config: &'a mut Value, keys: &[String]) -> Option<&'a mut Map<String, Value>> {
    keys.iter()
        .try_fold(config, |value, key| Some(value.as_object_mut()?.entry(key.as_str()).or_insert_with(|| json!({}))))?
        .as_object_mut()
}

/// The entry's `command` is a graphmind binary (`graphmind`, `graphmind.exe`, at any path).
pub(crate) fn points_at_graphmind(entry: &Value) -> bool {
    entry
        .get("command")
        .and_then(Value::as_str)
        .and_then(|command| Path::new(command).file_stem())
        .is_some_and(|stem| stem == SERVER_NAME)
}

/// `existing` equals `wanted`, treating an empty `env` object (which `claude mcp add` writes) as
/// no `env` at all.
pub(crate) fn is_current(existing: &Value, wanted: &Value) -> bool {
    let mut existing = existing.clone();
    if let Some(entry) = existing.as_object_mut() {
        if entry.get("env").and_then(Value::as_object).is_some_and(Map::is_empty) {
            entry.remove("env");
        }
    }
    existing == *wanted
}

/// Run the `claude` CLI (in `cwd` when given), killing it after `CLI_TIMEOUT`. The error is its
/// first stderr line.
fn run_claude(claude: &Path, cwd: Option<&Path>, args: &[&str]) -> Result<(), String> {
    let mut command = Command::new(claude);
    if let Some(cwd) = cwd {
        command.current_dir(cwd);
    }
    let mut child = command
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
    fn writing_a_config_backs_it_up_first() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("config.json");
        fs::write(&file, "{}").unwrap();
        write_config(&file, &json!({ "a": 1 })).unwrap();
        write_config(&file, &json!({ "a": 2 })).unwrap();
        assert_eq!(backups_in(dir.path()), 2);
    }

    #[test]
    fn claude_code_project_entry_lives_under_its_key() {
        let client = McpClient::ClaudeCodeProject(PathBuf::from("/repo/a"));
        assert_eq!(client.config_path(), McpClient::ClaudeCodeUser.config_path(), "same .claude.json");
        assert_eq!(McpClient::ClaudeCodeUser.claude_cli_scope(), Some(("user", None)));
        assert_eq!(client.claude_cli_scope(), Some(("local", Some(Path::new("/repo/a")))), "run in the key");
        assert_eq!(McpClient::Cursor.claude_cli_scope(), None);
        let submodule_worktree = McpClient::ClaudeCodeProject(PathBuf::from("/super/.git/modules/libs/sub"));
        assert_eq!(submodule_worktree.claude_cli_scope(), None, "a key inside .git is written directly");
        let bare = McpClient::ClaudeCodeProject(PathBuf::from("/repos/app.git"));
        assert_eq!(bare.claude_cli_scope(), Some(("local", Some(Path::new("/repos/app.git")))), "a bare repository");

        let mut config = json!({
            "numStartups": 3,
            "mcpServers": { "other": { "command": "other" } },
            "projects": { "/repo/a": { "allowedTools": [] }, "/repo/b": { "mcpServers": {} } }
        });
        let keys = client.servers_path();
        assert!(server_entry(&config, &keys).is_none());
        servers_mut(&mut config, &keys).unwrap().insert(SERVER_NAME.to_string(), client.entry("/bin/graphmind"));
        assert_eq!(
            config["projects"]["/repo/a"],
            json!({ "allowedTools": [], "mcpServers": { "graphmind": { "type": "stdio", "command": "/bin/graphmind", "args": ["mcp"] } } })
        );
        assert!(server_entry(&config, &McpClient::ClaudeCodeUser.servers_path()).is_none(), "not at user scope");
        assert_eq!(config["projects"]["/repo/b"], json!({ "mcpServers": {} }));

        let new_project = McpClient::ClaudeCodeProject(PathBuf::from("/repo/new")).servers_path();
        assert!(servers_mut(&mut config, &new_project).is_some(), "missing parents are created");
        assert!(servers_mut(&mut json!({ "projects": [] }), &keys).is_none(), "a non-object is not replaced");
    }

    #[test]
    fn only_entries_running_graphmind_count_as_graphmind_s() {
        assert!(points_at_graphmind(&json!({ "command": "/home/me/.graphmind/bin/graphmind", "args": ["mcp"] })));
        assert!(points_at_graphmind(&json!({ "command": "graphmind" })));
        assert!(points_at_graphmind(&json!({ "command": "C:/Users/me/.graphmind/bin/graphmind.exe" })));
        assert!(!points_at_graphmind(&json!({ "command": "/usr/local/bin/graphmind-wrapper" })));
        assert!(!points_at_graphmind(&json!({ "command": "npx", "args": ["graphmind"] })));
        assert!(!points_at_graphmind(&json!({ "url": "http://localhost:3000" })));
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

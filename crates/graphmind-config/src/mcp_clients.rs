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
    /// User scope in `.claude.json`, where `claude mcp add -s user` puts servers: in the
    /// `CLAUDE_CONFIG_DIR` folder held here, or in HOME for `None` (a folder that is HOME itself
    /// is `None` too, so one file has one value). [`McpClient::claude_code`] is the one this
    /// process's environment uses; [`claude_code_files`] lists every one uninstall covers.
    ClaudeCodeUser(Option<PathBuf>),
    Cursor,
    OpenCode,
    /// The project folder whose `.vscode/mcp.json` holds the entry.
    VsCode(PathBuf),
}

impl McpClient {
    /// Claude Code as this process's environment has it: `$CLAUDE_CONFIG_DIR/.claude.json` when
    /// the variable is an absolute path (`claude mcp add -s user` writes there then), else
    /// ~/.claude.json (see [`parse_claude_config_dir`]). config.json is never read: the desktop
    /// app sets the variable itself at startup (see [`inherit_recorded_claude_config_dir`]).
    pub fn claude_code() -> McpClient {
        McpClient::claude_code_in(claude_config_dir_env().dir().map(Path::to_path_buf))
    }

    /// Claude Code with `.claude.json` in `dir`, or in HOME for `None`.
    fn claude_code_in(dir: Option<PathBuf>) -> McpClient {
        let home: PathBuf = home_dir().components().collect();
        McpClient::ClaudeCodeUser(dir.filter(|dir| *dir != home))
    }

    pub fn label(&self) -> &'static str {
        match self {
            McpClient::ClaudeDesktop => "Claude Desktop",
            McpClient::ClaudeCodeUser(_) => "Claude Code",
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
            McpClient::ClaudeCodeUser(dir) => dir.clone().unwrap_or_else(home_dir).join(".claude.json"),
            McpClient::Cursor => home_dir().join(".cursor").join("mcp.json"),
            McpClient::OpenCode => home_dir().join(".config/opencode/opencode.jsonc"),
            McpClient::VsCode(project) => project.join(".vscode").join("mcp.json"),
        }
    }

    /// Whether the client is in use here. Apps create their config folder on first run, so a
    /// missing folder means a missing app: graphmind must not create folders for it.
    pub fn is_installed(&self) -> bool {
        match self {
            // graphmind's hooks and skills target Claude Code, and the client's .claude.json needs
            // no app folder (it lives in HOME or `CLAUDE_CONFIG_DIR`).
            McpClient::ClaudeCodeUser(_) => true,
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
            McpClient::ClaudeDesktop | McpClient::ClaudeCodeUser(_) | McpClient::Cursor => "mcpServers",
            McpClient::OpenCode => "mcp",
            McpClient::VsCode(_) => "servers",
        }
    }

    /// The exact entry this client should hold. No `env` block: `bin` is absolute, and an
    /// `env.PATH` would replace the inherited environment (nvm, asdf, mise...). See issue #105.
    fn entry(&self, bin: &str) -> Value {
        match self {
            McpClient::ClaudeDesktop | McpClient::Cursor => json!({ "command": bin, "args": ["mcp"] }),
            McpClient::ClaudeCodeUser(_) | McpClient::VsCode(_) => {
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

/// Every Claude Code `.claude.json` graphmind may have written to, for uninstall and the cleanup
/// of old project-scope entries: this process's ([`McpClient::claude_code`]), ~/.claude.json
/// (Claude Code reads it whenever it runs without the variable) and the recorded folder's (the
/// desktop app's, see [`recorded_claude_config_dir`]). Removal only touches graphmind's own keys,
/// so going through files that hold none changes nothing.
pub fn claude_code_files() -> Vec<McpClient> {
    let candidates = [
        Some(McpClient::claude_code()),
        Some(McpClient::claude_code_in(None)),
        recorded_claude_config_dir().map(|dir| McpClient::claude_code_in(Some(dir))),
    ];
    let mut files: Vec<McpClient> = Vec::new();
    for client in candidates.into_iter().flatten() {
        if !files.iter().any(|file| file.config_path() == client.config_path()) {
            files.push(client);
        }
    }
    files
}

pub const CLAUDE_CONFIG_DIR: &str = "CLAUDE_CONFIG_DIR";

/// `CLAUDE_CONFIG_DIR` as a process sees it. See [`parse_claude_config_dir`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConfigDirEnv {
    /// Not set.
    Unset,
    /// Set but empty: Claude Code's default folder.
    Default,
    /// Set to an absolute path (without trailing `/` or `.` segments, so one folder has one value).
    Dir(PathBuf),
    /// Set to a relative path, which graphmind ignores: Claude Code would resolve it against the
    /// directory it starts in, and hooks and the MCP server run in each project's directory.
    Ignored(OsString),
}

impl ConfigDirEnv {
    /// The folder the variable names, for [`ConfigDirEnv::Dir`] only.
    pub fn dir(&self) -> Option<&Path> {
        match self {
            ConfigDirEnv::Dir(dir) => Some(dir),
            ConfigDirEnv::Unset | ConfigDirEnv::Default | ConfigDirEnv::Ignored(_) => None,
        }
    }
}

/// The one reading of a `CLAUDE_CONFIG_DIR` value: not set, empty (the default folder), an
/// absolute path (that folder) or a relative one (ignored).
pub fn parse_claude_config_dir(value: Option<OsString>) -> ConfigDirEnv {
    match value {
        None => ConfigDirEnv::Unset,
        Some(value) if value.is_empty() => ConfigDirEnv::Default,
        Some(value) if Path::new(&value).is_absolute() => {
            ConfigDirEnv::Dir(Path::new(&value).components().collect())
        }
        Some(value) => ConfigDirEnv::Ignored(value),
    }
}

/// `CLAUDE_CONFIG_DIR` as this process sees it.
pub fn claude_config_dir_env() -> ConfigDirEnv {
    parse_claude_config_dir(std::env::var_os(CLAUDE_CONFIG_DIR))
}

/// The `CLAUDE_CONFIG_DIR` setup recorded in config.json for processes that don't inherit it,
/// while the folder exists.
pub fn recorded_claude_config_dir() -> Option<PathBuf> {
    crate::read_config().ok()?.claude_config_dir.filter(|dir| dir.is_dir())
}

/// For the desktop app, which doesn't inherit `CLAUDE_CONFIG_DIR` when started from the Finder:
/// when the variable is unset or empty, set it to the folder setup recorded, while that folder
/// exists. Everything the process does afterwards goes by the variable, like the CLI, and every
/// graphmind command it starts inherits it. Returns the folder set, if any.
///
/// Changing the environment isn't thread-safe: call this before any thread starts.
pub fn inherit_recorded_claude_config_dir() -> Option<PathBuf> {
    if !matches!(claude_config_dir_env(), ConfigDirEnv::Unset | ConfigDirEnv::Default) {
        return None;
    }
    let dir = recorded_claude_config_dir()?;
    std::env::set_var(CLAUDE_CONFIG_DIR, &dir);
    Some(dir)
}

/// What [`sync_claude_config_dir`] did with the folder recorded in config.json.
#[derive(Debug, PartialEq, Eq)]
pub enum RecordedConfigDir {
    /// Nothing to change.
    Unchanged,
    /// `CLAUDE_CONFIG_DIR` isn't set here, or is relative, so this process configures
    /// ~/.claude.json, while the desktop app keeps using the recorded folder (it still exists).
    Kept(PathBuf),
    Recorded(PathBuf),
    /// The previously recorded folder, no longer used.
    Cleared(PathBuf),
}

/// Record `CLAUDE_CONFIG_DIR` in config.json for the desktop app (see
/// [`inherit_recorded_claude_config_dir`]). Setup runs this. `Err` when config.json can't be
/// read: it is left as is, like every other save.
pub fn sync_claude_config_dir() -> Result<RecordedConfigDir, String> {
    let env = claude_config_dir_env();
    crate::config::mutate_config(|config| {
        let previous = config.claude_config_dir.clone();
        let wanted = recorded_dir_to_keep(&env, previous.clone());
        let result = match (&wanted, previous) {
            (Some(dir), previous) if previous.as_ref() != Some(dir) => RecordedConfigDir::Recorded(dir.clone()),
            (None, Some(previous)) => RecordedConfigDir::Cleared(previous),
            (Some(dir), _) if env.dir().is_none() => return (RecordedConfigDir::Kept(dir.clone()), false),
            _ => return (RecordedConfigDir::Unchanged, false),
        };
        config.claude_config_dir = wanted;
        (result, true)
    })
    .map_err(|e| format!("{e}; CLAUDE_CONFIG_DIR not recorded"))
}

/// The folder to keep recorded, given `CLAUDE_CONFIG_DIR` as this process sees it:
/// - an absolute path: that path, the folder Claude Code uses in this environment;
/// - empty: none, Claude Code uses ~/.claude.json (`CLAUDE_CONFIG_DIR= graphmind setup` is how
///   to drop a recorded folder that still exists);
/// - not set, or relative (ignored): the recorded folder while it exists. Such a shell says
///   nothing about the environment Claude Code runs in; a folder that is gone can't be the one
///   it uses.
fn recorded_dir_to_keep(env: &ConfigDirEnv, recorded: Option<PathBuf>) -> Option<PathBuf> {
    match env {
        ConfigDirEnv::Default => None,
        ConfigDirEnv::Dir(dir) => Some(dir.clone()),
        ConfigDirEnv::Unset | ConfigDirEnv::Ignored(_) => recorded.filter(|dir| dir.is_dir()),
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
    if matches!(client, McpClient::ClaudeCodeUser(_)) {
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
    if matches!(client, McpClient::ClaudeCodeUser(_)) {
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
        Some(_) => run_claude(claude, client, &["mcp", "remove", "-s", "user", SERVER_NAME]),
        None => Ok(()),
    }
    .and_then(|()| run_claude(claude, client, &["mcp", "add", "-s", "user", SERVER_NAME, "--", bin, "mcp"]));
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
    match run_claude(claude, client, &["mcp", "remove", "-s", "user", SERVER_NAME]) {
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
/// `claude_code`'s .claude.json) that older versions wrote, for one project or, with `None`, for
/// all of them. Returns how many were removed. Callers go through [`claude_code_files`].
pub fn remove_claude_project_entries(claude_code: &McpClient, project: Option<&str>) -> Result<usize, String> {
    let path = claude_code.config_path();
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
        crate::back_up(path)?;
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

/// Run the `claude` CLI for `client`, killing it after `CLI_TIMEOUT`. The error is its first
/// stderr line. The CLI gets the folder `client`'s config is in as `CLAUDE_CONFIG_DIR`, so it
/// writes the file graphmind checks afterwards, also for a file other than this process's (see
/// [`claude_code_files`]). For ~/.claude.json the variable is removed, so an empty or relative
/// one doesn't depend on how `claude` reads it.
fn run_claude(claude: &Path, client: &McpClient, args: &[&str]) -> Result<(), String> {
    let mut command = Command::new(claude);
    match client {
        McpClient::ClaudeCodeUser(Some(dir)) => command.env("CLAUDE_CONFIG_DIR", dir),
        _ => command.env_remove("CLAUDE_CONFIG_DIR"),
    };
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

    /// Empty means the default folder, absolute means that folder, relative is ignored; a folder
    /// is the same value however it is spelled, and a folder that is HOME means ~/.claude.json.
    #[test]
    fn claude_config_dir_has_one_reading() {
        let parse = |value: &str| parse_claude_config_dir(Some(OsString::from(value)));
        assert_eq!(parse_claude_config_dir(None), ConfigDirEnv::Unset);
        assert_eq!(parse(""), ConfigDirEnv::Default);
        assert_eq!(parse("/cfg/claude"), ConfigDirEnv::Dir(PathBuf::from("/cfg/claude")));
        assert_eq!(parse("/cfg/./claude/"), ConfigDirEnv::Dir(PathBuf::from("/cfg/claude")));
        assert_eq!(parse("claude-config"), ConfigDirEnv::Ignored(OsString::from("claude-config")));
        assert_eq!(parse("claude-config").dir(), None);
        assert_eq!(McpClient::claude_code_in(Some(home_dir())), McpClient::ClaudeCodeUser(None));
    }

    /// The desktop app takes the recorded folder when it has no `CLAUDE_CONFIG_DIR` of its own
    /// (unset or empty) and the folder exists; a variable it has is kept.
    #[test]
    fn desktop_inherits_the_recorded_config_dir_only_without_its_own() {
        let _home = crate::config::TEST_HOME_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let home = tempfile::tempdir().unwrap();
        unsafe { std::env::set_var("HOME", home.path()) };
        crate::config::ensure_dirs();
        let recorded = home.path().join("claude-config");
        fs::create_dir(&recorded).unwrap();
        let config = crate::GlobalConfig { claude_config_dir: Some(recorded.clone()), ..Default::default() };
        crate::save_config(&config);
        let inherit = |env: Option<&str>| {
            match env {
                Some(value) => unsafe { std::env::set_var(CLAUDE_CONFIG_DIR, value) },
                None => unsafe { std::env::remove_var(CLAUDE_CONFIG_DIR) },
            }
            let set = inherit_recorded_claude_config_dir();
            let path = McpClient::claude_code().config_path();
            unsafe { std::env::remove_var(CLAUDE_CONFIG_DIR) };
            (set, path)
        };

        let taken = (Some(recorded.clone()), recorded.join(".claude.json"));
        assert_eq!(inherit(None), taken);
        assert_eq!(inherit(Some("")), taken);
        assert_eq!(inherit(Some("/cfg/claude")), (None, PathBuf::from("/cfg/claude/.claude.json")));
        assert_eq!(inherit(Some("claude-config")), (None, home.path().join(".claude.json")));
        fs::remove_dir(&recorded).unwrap();
        assert_eq!(inherit(None), (None, home.path().join(".claude.json")), "the recorded folder is gone");
    }

    #[test]
    fn recorded_dir_is_cleared_only_when_gone_or_explicitly_empty() {
        let existing = tempfile::tempdir().unwrap();
        let existing = Some(existing.path().to_path_buf());
        let gone = Some(PathBuf::from("/nonexistent/claude-config"));
        let set = |dir: &str| parse_claude_config_dir(Some(OsString::from(dir)));
        let unset = ConfigDirEnv::Unset;

        assert_eq!(recorded_dir_to_keep(&set("/cfg/new"), existing.clone()), Some(PathBuf::from("/cfg/new")));
        assert_eq!(recorded_dir_to_keep(&set("/cfg/new"), None), Some(PathBuf::from("/cfg/new")));
        // Unset: a shell without the variable keeps an existing folder for the desktop app.
        assert_eq!(recorded_dir_to_keep(&unset, existing.clone()), existing);
        assert_eq!(recorded_dir_to_keep(&unset, gone.clone()), None);
        assert_eq!(recorded_dir_to_keep(&unset, None), None);
        // Set but empty: back to ~/.claude.json.
        assert_eq!(recorded_dir_to_keep(&set(""), existing.clone()), None);
        // Relative: ignored, like unset.
        assert_eq!(recorded_dir_to_keep(&set("claude-config"), existing.clone()), existing);
        assert_eq!(recorded_dir_to_keep(&set("claude-config"), gone), None);
        assert_eq!(recorded_dir_to_keep(&set("claude-config"), None), None);
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

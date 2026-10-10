//! The Claude Code folder graphmind sets up, [`ClaudeHome`]: ~/.claude, or the folder
//! `CLAUDE_CONFIG_DIR` names. Claude Code keeps settings.json (with the hooks), hooks/, skills/
//! and CLAUDE.md there, and its user-scope MCP servers in `.claude.json`. Also here: the one
//! reading of the variable ([`parse_claude_config_dir`]), the folder setup records for the desktop
//! app, and each folder's setup state in config.json.

use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};

use crate::config::{mutate_config, GlobalConfig};
use crate::mcp_clients::McpClient;
use crate::paths::home_dir;

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

/// A Claude Code configuration folder: ~/.claude, or the folder an absolute `CLAUDE_CONFIG_DIR`
/// names. Several can be in use (one per account profile, say), and each is set up on its own.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClaudeHome {
    /// The folder, `None` for ~/.claude, also when the variable names ~/.claude: one folder has
    /// one setup-state key.
    config_dir: Option<PathBuf>,
    /// The folder's user-scope MCP config. `.claude.json` moves with the variable even when it
    /// names ~/.claude, so this isn't derived from `config_dir`.
    mcp_client: McpClient,
    /// A relative `CLAUDE_CONFIG_DIR`, which graphmind ignores (see [`ConfigDirEnv::Ignored`]).
    ignored: Option<OsString>,
}

impl ClaudeHome {
    /// ~/.claude, with ~/.claude.json: what Claude Code uses without `CLAUDE_CONFIG_DIR`.
    pub fn default_home() -> ClaudeHome {
        ClaudeHome::named(None)
    }

    /// The folder Claude Code uses in this process's environment. A hook or MCP server started by
    /// Claude Code inherits its session's `CLAUDE_CONFIG_DIR`, so it works on that session's
    /// folder. config.json is never read: the desktop app sets the variable itself at startup
    /// (see [`inherit_recorded_claude_config_dir`]).
    pub fn current() -> ClaudeHome {
        ClaudeHome::from_env(claude_config_dir_env())
    }

    fn from_env(env: ConfigDirEnv) -> ClaudeHome {
        let mut home = ClaudeHome::named(env.dir().map(Path::to_path_buf));
        if let ConfigDirEnv::Ignored(value) = env {
            home.ignored = Some(value);
        }
        home
    }

    /// The folder `CLAUDE_CONFIG_DIR=<dir>` sets up, ~/.claude for `None`. `dir` is spelled as
    /// [`parse_claude_config_dir`] and config.json hold it.
    fn named(dir: Option<PathBuf>) -> ClaudeHome {
        let default: PathBuf = home_dir().join(".claude").components().collect();
        ClaudeHome {
            config_dir: dir.clone().filter(|dir| *dir != default),
            mcp_client: claude_code_in(dir),
            ignored: None,
        }
    }

    /// The `CLAUDE_CONFIG_DIR` folder, `None` for ~/.claude.
    pub fn config_dir(&self) -> Option<&Path> {
        self.config_dir.as_deref()
    }

    pub fn is_default(&self) -> bool {
        self.config_dir.is_none()
    }

    /// The relative `CLAUDE_CONFIG_DIR` this process has and graphmind ignores, if any.
    pub fn ignored(&self) -> Option<&OsStr> {
        self.ignored.as_deref()
    }

    /// The folder itself.
    pub fn dir(&self) -> PathBuf {
        self.config_dir.clone().unwrap_or_else(|| home_dir().join(".claude"))
    }

    pub fn settings_path(&self) -> PathBuf {
        self.dir().join("settings.json")
    }

    /// Where graphmind's hook scripts go: next to the settings.json that registers them, so each
    /// folder's hooks are installed and removed with it.
    pub fn hooks_dir(&self) -> PathBuf {
        self.dir().join("hooks")
    }

    pub fn skills_dir(&self) -> PathBuf {
        self.dir().join("skills")
    }

    pub fn claude_md_path(&self) -> PathBuf {
        self.dir().join("CLAUDE.md")
    }

    /// Claude Code's user-scope MCP config for this folder.
    pub fn mcp_client(&self) -> McpClient {
        self.mcp_client.clone()
    }

    /// The version of the last setup completed for this folder (0: never, or uninstalled).
    /// ~/.claude's is `setup_version`, the field older versions and the desktop app read; the
    /// other folders' are in `setup_versions`.
    pub fn setup_version(&self, config: &GlobalConfig) -> u32 {
        match &self.config_dir {
            None => config.setup_version,
            Some(dir) => config.setup_versions.get(&*dir.to_string_lossy()).copied().unwrap_or(0),
        }
    }

    /// Record `version` as this folder's setup version (0 forgets it), and drop the records of
    /// folders that no longer exist, which are returned. Under the config lock: setups of
    /// different folders can run at once (auto-setup from two sessions), and neither may drop
    /// the other's record. `Err` when config.json can't be read: it is left as is.
    pub fn record_setup_version(&self, version: u32) -> Result<Vec<PathBuf>, String> {
        mutate_config(|config| {
            let before = (config.setup_version, config.setup_versions.clone());
            let gone: Vec<PathBuf> =
                config.setup_versions.keys().map(PathBuf::from).filter(|dir| !dir.is_dir()).collect();
            for dir in &gone {
                config.setup_versions.remove(&*dir.to_string_lossy());
            }
            match &self.config_dir {
                None => config.setup_version = version,
                Some(dir) if version == 0 => {
                    config.setup_versions.remove(&*dir.to_string_lossy());
                }
                Some(dir) => {
                    config.setup_versions.insert(dir.to_string_lossy().into_owned(), version);
                }
            }
            let changed = config.setup_version != before.0 || config.setup_versions != before.1;
            (gone, changed)
        })
        .map_err(|e| format!("{e}; setup state not recorded"))
    }

    /// Every folder graphmind may have set up, each once, for `uninstall all`: this process's,
    /// ~/.claude (Claude Code uses it whenever it runs without the variable), the recorded one (the
    /// desktop app's) and every folder in `setup_versions` that still exists.
    pub fn all_known() -> Vec<ClaudeHome> {
        let mut homes: Vec<ClaudeHome> = Vec::new();
        for home in known_homes() {
            if !homes.iter().any(|known| known.dir() == home.dir()) {
                homes.push(home);
            }
        }
        homes
    }
}

impl McpClient {
    /// Claude Code as this process's environment has it: `$CLAUDE_CONFIG_DIR/.claude.json` when
    /// the variable is an absolute path (`claude mcp add -s user` writes there then), else
    /// ~/.claude.json. See [`ClaudeHome::current`].
    pub fn claude_code() -> McpClient {
        ClaudeHome::current().mcp_client
    }
}

/// Claude Code with `.claude.json` in `dir`, or in HOME for `None`. A folder that is HOME itself is
/// `None` too, so one file has one value.
fn claude_code_in(dir: Option<PathBuf>) -> McpClient {
    let home: PathBuf = home_dir().components().collect();
    McpClient::ClaudeCodeUser(dir.filter(|dir| *dir != home))
}

/// Every Claude Code `.claude.json` graphmind may have written to, for uninstall and the cleanup
/// of old project-scope entries: those of the folders [`ClaudeHome::all_known`] lists, this
/// process's first ([`McpClient::claude_code`]). Removal only touches graphmind's own keys, so
/// going through files that hold none changes nothing.
pub fn claude_code_files() -> Vec<McpClient> {
    let mut files: Vec<McpClient> = Vec::new();
    for client in known_homes().into_iter().map(|home| home.mcp_client) {
        if !files.iter().any(|file| file.config_path() == client.config_path()) {
            files.push(client);
        }
    }
    files
}

/// This process's folder, ~/.claude, the recorded folder and the folders in `setup_versions`
/// that still exist, in that order (repeats included).
fn known_homes() -> Vec<ClaudeHome> {
    let config = crate::read_config().unwrap_or_default();
    let recorded = config.claude_config_dir.filter(|dir| dir.is_dir());
    let set_up = config.setup_versions.into_keys().map(PathBuf::from).filter(|dir| dir.is_dir());
    [ClaudeHome::current(), ClaudeHome::default_home()]
        .into_iter()
        .chain(recorded.into_iter().chain(set_up).map(|dir| ClaudeHome::named(Some(dir))))
        .collect()
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
    /// `CLAUDE_CONFIG_DIR` isn't set here, or is relative, so this process sets up ~/.claude,
    /// while the desktop app keeps using the recorded folder (it still exists).
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
    mutate_config(|config| {
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
/// - empty: none, Claude Code uses ~/.claude (`CLAUDE_CONFIG_DIR= graphmind setup` is how to drop
///   a recorded folder that still exists);
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn parse(value: &str) -> ConfigDirEnv {
        parse_claude_config_dir(Some(OsString::from(value)))
    }

    /// Points HOME at a fresh temp dir with an empty ~/.graphmind for `f`, under the lock every
    /// test that changes HOME holds. CLAUDE_CONFIG_DIR is removed before and after.
    fn with_temp_home(f: impl FnOnce(&Path)) {
        let _home = crate::config::TEST_HOME_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let home = tempfile::tempdir().unwrap();
        unsafe {
            std::env::set_var("HOME", home.path());
            std::env::remove_var(CLAUDE_CONFIG_DIR);
        }
        crate::config::ensure_dirs();
        f(home.path());
        unsafe { std::env::remove_var(CLAUDE_CONFIG_DIR) };
    }

    /// Empty means the default folder, absolute means that folder, relative is ignored; a folder
    /// is the same value however it is spelled, and a folder that is HOME means ~/.claude.json.
    #[test]
    fn claude_config_dir_has_one_reading() {
        assert_eq!(parse_claude_config_dir(None), ConfigDirEnv::Unset);
        assert_eq!(parse(""), ConfigDirEnv::Default);
        assert_eq!(parse("/cfg/claude"), ConfigDirEnv::Dir(PathBuf::from("/cfg/claude")));
        assert_eq!(parse("/cfg/./claude/"), ConfigDirEnv::Dir(PathBuf::from("/cfg/claude")));
        assert_eq!(parse("claude-config"), ConfigDirEnv::Ignored(OsString::from("claude-config")));
        assert_eq!(parse("claude-config").dir(), None);
        with_temp_home(|home| {
            assert_eq!(claude_code_in(Some(home.to_path_buf())), McpClient::ClaudeCodeUser(None));
        });
    }

    /// Everything setup installs for Claude Code follows CLAUDE_CONFIG_DIR: settings.json (hooks),
    /// hooks/, skills/, CLAUDE.md and .claude.json. Unset, empty or relative, they stay in
    /// ~/.claude and ~/.claude.json, and a relative value is kept to be reported.
    #[test]
    fn claude_home_puts_everything_in_the_config_dir() {
        with_temp_home(|home| {
            let dir = ClaudeHome::from_env(parse("/cfg/claude/"));
            assert_eq!(dir.config_dir(), Some(Path::new("/cfg/claude")), "one folder, one key");
            assert!(!dir.is_default());
            assert_eq!(dir.settings_path(), Path::new("/cfg/claude/settings.json"));
            assert_eq!(dir.hooks_dir(), Path::new("/cfg/claude/hooks"));
            assert_eq!(dir.skills_dir(), Path::new("/cfg/claude/skills"));
            assert_eq!(dir.claude_md_path(), Path::new("/cfg/claude/CLAUDE.md"));
            assert_eq!(dir.mcp_client().config_path(), Path::new("/cfg/claude/.claude.json"));
            assert_eq!(dir.ignored(), None);

            let default = ClaudeHome::default_home();
            for env in [ConfigDirEnv::Unset, parse("")] {
                assert_eq!(ClaudeHome::from_env(env), default);
            }
            assert_eq!(default.settings_path(), home.join(".claude/settings.json"));
            assert_eq!(default.hooks_dir(), home.join(".claude/hooks"));
            assert_eq!(default.mcp_client().config_path(), home.join(".claude.json"));

            let relative = ClaudeHome::from_env(parse("relative"));
            assert_eq!(relative.ignored(), Some(OsStr::new("relative")));
            assert!(relative.is_default());
            assert_eq!(relative.mcp_client(), default.mcp_client());
        });
    }

    /// `CLAUDE_CONFIG_DIR=$HOME/.claude` is ~/.claude: one setup-state key (`setup_version`), so
    /// uninstall resets the version setup stamped, while `.claude.json` stays where Claude Code
    /// reads it with that variable, inside the folder.
    #[test]
    fn the_default_folder_named_by_the_variable_is_the_default() {
        with_temp_home(|home| {
            let named = ClaudeHome::from_env(parse(&home.join(".claude/").to_string_lossy()));
            assert!(named.is_default());
            assert_eq!(named.dir(), home.join(".claude"));
            assert_eq!(named.mcp_client().config_path(), home.join(".claude/.claude.json"));

            named.record_setup_version(7).unwrap();
            let config = crate::read_config().unwrap();
            assert_eq!(config.setup_version, 7);
            assert!(config.setup_versions.is_empty(), "{:?}", config.setup_versions);
            assert_eq!(ClaudeHome::default_home().setup_version(&config), 7);
            named.record_setup_version(0).unwrap();
            assert_eq!(crate::read_config().unwrap().setup_version, 0);
        });
    }

    /// `uninstall all` goes through each folder graphmind may have set up once: ~/.claude also when
    /// the variable names it, the recorded folder and every set-up folder that still exists. The
    /// `.claude.json` list keeps both of ~/.claude's files, in HOME and inside the folder.
    #[test]
    fn every_known_folder_once() {
        with_temp_home(|home| {
            let recorded = home.join("profiles/recorded");
            let work = home.join("profiles/work");
            fs::create_dir_all(&recorded).unwrap();
            fs::create_dir_all(&work).unwrap();
            let mut config = GlobalConfig { claude_config_dir: Some(recorded.clone()), ..Default::default() };
            for dir in [&recorded, &work, &home.join("profiles/gone")] {
                config.setup_versions.insert(dir.to_string_lossy().into_owned(), 1);
            }
            crate::save_config(&config);
            unsafe { std::env::set_var(CLAUDE_CONFIG_DIR, home.join(".claude")) };

            let dirs: Vec<PathBuf> = ClaudeHome::all_known().iter().map(ClaudeHome::dir).collect();
            assert_eq!(dirs, [home.join(".claude"), recorded.clone(), work.clone()]);
            let files: Vec<PathBuf> = claude_code_files().iter().map(McpClient::config_path).collect();
            assert_eq!(
                files,
                [home.join(".claude/.claude.json"), home.join(".claude.json"), recorded.join(".claude.json"), work.join(".claude.json")]
            );
        });
    }

    /// Each folder has its own version; 0 forgets a folder, and the records of folders that are
    /// gone are dropped (and returned) whenever a version is recorded.
    #[test]
    fn setup_versions_are_per_folder_and_pruned() {
        with_temp_home(|home| {
            let work = home.join("work");
            let gone = home.join("gone");
            fs::create_dir_all(&work).unwrap();
            fs::create_dir_all(&gone).unwrap();
            let work_home = ClaudeHome::named(Some(work.clone()));
            let gone_home = ClaudeHome::named(Some(gone.clone()));

            assert_eq!(work_home.record_setup_version(5), Ok(vec![]));
            assert_eq!(gone_home.record_setup_version(5), Ok(vec![]));
            let config = crate::read_config().unwrap();
            assert_eq!(work_home.setup_version(&config), 5);
            assert_eq!(ClaudeHome::default_home().setup_version(&config), 0, "another folder marked ~/.claude");

            fs::remove_dir(&gone).unwrap();
            assert_eq!(ClaudeHome::default_home().record_setup_version(5), Ok(vec![gone.clone()]));
            let config = crate::read_config().unwrap();
            assert_eq!(config.setup_versions.keys().collect::<Vec<_>>(), [&*work.to_string_lossy()]);
            assert_eq!(config.setup_version, 5);

            assert_eq!(work_home.record_setup_version(0), Ok(vec![]));
            assert!(crate::read_config().unwrap().setup_versions.is_empty());
        });
    }

    /// An unreadable config.json is left as is, and the caller hears about it.
    #[test]
    fn record_setup_version_reports_an_unreadable_config() {
        with_temp_home(|home| {
            let config_json = home.join(".graphmind/config.json");
            fs::write(&config_json, "{\"projects\": {").unwrap();
            assert!(ClaudeHome::default_home().record_setup_version(5).is_err());
            assert_eq!(fs::read_to_string(&config_json).unwrap(), "{\"projects\": {");
        });
    }

    /// The desktop app takes the recorded folder when it has no `CLAUDE_CONFIG_DIR` of its own
    /// (unset or empty) and the folder exists; a variable it has is kept.
    #[test]
    fn desktop_inherits_the_recorded_config_dir_only_without_its_own() {
        with_temp_home(|home| {
            let recorded = home.join("claude-config");
            fs::create_dir(&recorded).unwrap();
            let config = GlobalConfig { claude_config_dir: Some(recorded.clone()), ..Default::default() };
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
            assert_eq!(inherit(Some("claude-config")), (None, home.join(".claude.json")));
            fs::remove_dir(&recorded).unwrap();
            assert_eq!(inherit(None), (None, home.join(".claude.json")), "the recorded folder is gone");
        });
    }

    #[test]
    fn recorded_dir_is_cleared_only_when_gone_or_explicitly_empty() {
        let existing = tempfile::tempdir().unwrap();
        let existing = Some(existing.path().to_path_buf());
        let gone = Some(PathBuf::from("/nonexistent/claude-config"));
        let unset = ConfigDirEnv::Unset;

        assert_eq!(recorded_dir_to_keep(&parse("/cfg/new"), existing.clone()), Some(PathBuf::from("/cfg/new")));
        assert_eq!(recorded_dir_to_keep(&parse("/cfg/new"), None), Some(PathBuf::from("/cfg/new")));
        // Unset: a shell without the variable keeps an existing folder for the desktop app.
        assert_eq!(recorded_dir_to_keep(&unset, existing.clone()), existing);
        assert_eq!(recorded_dir_to_keep(&unset, gone.clone()), None);
        assert_eq!(recorded_dir_to_keep(&unset, None), None);
        // Set but empty: back to ~/.claude.
        assert_eq!(recorded_dir_to_keep(&parse(""), existing.clone()), None);
        // Relative: ignored, like unset.
        assert_eq!(recorded_dir_to_keep(&parse("claude-config"), existing.clone()), existing);
        assert_eq!(recorded_dir_to_keep(&parse("claude-config"), gone), None);
        assert_eq!(recorded_dir_to_keep(&parse("claude-config"), None), None);
    }
}

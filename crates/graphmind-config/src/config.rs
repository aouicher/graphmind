use crate::{git_identity, paths};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "snake_case")]
pub enum Tier {
    #[default]
    Free,
    Embeddings,
    Pro,
    Team,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Feature {
    // Free — always available
    LocalGraph,
    LocalMcp,
    LocalMemory,
    LocalEmbeddings,
    // Embeddings tier
    RemoteEmbeddings,
    SemanticSearch,
    // Pro tier
    RemoteApi,
    RemoteMcp,
    // Team tier
    TeamSync,
    TeamMemories,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct LicenseConfig {
    pub key: Option<String>,
    #[serde(skip)]
    pub cached_tier: Option<Tier>,
    pub last_validated_at: Option<u64>,
}

/// Bump this when hooks, skills, or MCP config format changes.
/// Users with a lower stored version get a "run graphmind setup" warning.
///
/// v3 — issue #105: MCP entries no longer carry an `env.PATH` block, and the
/// global `env.PATH` in ~/.claude/settings.json is removed. Bumping this makes
/// `auto_setup_if_outdated()` run the repair for already-installed users.
///
/// v4 — git worktree support: setup adds the post-checkout / post-merge /
/// post-rewrite git hooks to repos that already have graphmind's hooks.
///
/// v5 — the Claude Code MCP server is registered at user scope (~/.claude.json) only: setup
/// removes the old entries from ~/.claude/settings.json and the per-project
/// `projects.*.mcpServers.graphmind` entries once the user-scope entry is in place.
pub const SETUP_VERSION: u32 = 5;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpConfig {
    pub transport: String,
    pub http_port: u16,
    pub restrict_to_projects: Option<Vec<String>>,
}

impl Default for McpConfig {
    fn default() -> Self {
        Self {
            transport: "stdio".to_string(),
            http_port: 37378,
            restrict_to_projects: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DefaultsConfig {
    pub embedding_model: String,
    pub watch_debounce: u64,
    pub max_depth: u32,
    pub exclude_tests: bool,
}

impl Default for DefaultsConfig {
    fn default() -> Self {
        Self {
            embedding_model: "minilm".to_string(),
            watch_debounce: 2000,
            max_depth: 5,
            exclude_tests: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectConfig {
    pub path: String,
    pub slug: String,
    pub registered: String,
    pub last_build: Option<String>,
    pub auto_watch: bool,
    pub languages: Vec<String>,
    pub exclude: Vec<String>,
    /// Identity shared by every worktree of the same logical git repo.
    /// `None` for projects not inside a git repo (or registered before
    /// this field existed) — such projects keep pure path-based behavior.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repo_id: Option<String>,
    /// Metadata only, refreshed on build — never a storage key.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub branch: Option<String>,
    /// For a worktree of an already-registered repo: the slug of the project
    /// whose memory it shares. `None` for every project registered before
    /// worktree support, which keeps its own memory exactly as before.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub primary: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum EmbeddingMode {
    Local,
    Openai,
    Voyage,
    #[default]
    Disabled,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ApiKeys {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub openai: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub voyage: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmbeddingConfig {
    pub mode: EmbeddingMode,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub openai_base_url: Option<String>,
    #[serde(default)]
    pub api_keys: ApiKeys,
    /// Per-request timeout for embedding APIs, in seconds. Default: 30, or 300 when
    /// `openai_base_url` points at this machine (a local model server such as Ollama).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub request_timeout_secs: Option<u64>,
    /// Symbols sent per embedding request during builds. Default: 256.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub batch_size: Option<usize>,
    /// Text put in front of search queries before embedding. Meant for models trained with an
    /// instruction on the query side only, such as Qwen3-Embedding or F2LLM (an
    /// "Instruct: ...\nQuery: " line). Indexed text is never prefixed, so it is not a fit for
    /// models trained with paired prefixes (nomic-embed-text's `search_query:`/`search_document:`,
    /// e5's `query:`/`passage:`): a query-only prefix there may not help. Leave unset otherwise.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub query_prefix: Option<String>,
}

impl Default for EmbeddingConfig {
    fn default() -> Self {
        Self {
            mode: EmbeddingMode::Disabled,
            model: None,
            openai_base_url: None,
            api_keys: ApiKeys::default(),
            request_timeout_secs: None,
            batch_size: None,
            query_prefix: None,
        }
    }
}

impl EmbeddingConfig {
    /// Symbols per embedding request during builds (at least 1).
    pub fn effective_batch_size(&self) -> usize {
        self.batch_size.unwrap_or(256).max(1)
    }
}

/// Memory settings.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct MemoryConfig {
    /// Promote an entry to priority once it has been recalled this many times by explicit
    /// searches (`memory search`, `gm_memory_search`). Default 3; 0 turns auto-promotion off.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auto_promote_min_recalls: Option<u32>,
}

impl MemoryConfig {
    /// Recall count that triggers auto-promotion, or `None` when it is off.
    pub fn auto_promote_threshold(&self) -> Option<std::num::NonZeroU32> {
        std::num::NonZeroU32::new(self.auto_promote_min_recalls.unwrap_or(3))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GlobalConfig {
    pub version: String,
    pub projects: HashMap<String, ProjectConfig>,
    pub global_exclude: Vec<String>,
    pub defaults: DefaultsConfig,
    pub mcp: McpConfig,
    #[serde(default)]
    pub embedding: EmbeddingConfig,
    #[serde(default)]
    pub setup_version: u32,
    #[serde(default)]
    pub license: LicenseConfig,
    #[serde(default)]
    pub launch_at_login: bool,
    #[serde(default)]
    pub build_all_on_startup: bool,
    #[serde(default)]
    pub memory: MemoryConfig,
}

impl Default for GlobalConfig {
    fn default() -> Self {
        Self {
            version: "1".to_string(),
            projects: HashMap::new(),
            global_exclude: Vec::new(),
            defaults: DefaultsConfig::default(),
            mcp: McpConfig::default(),
            embedding: EmbeddingConfig::default(),
            setup_version: 0,
            license: LicenseConfig::default(),
            launch_at_login: false,
            build_all_on_startup: false,
            memory: MemoryConfig::default(),
        }
    }
}

pub fn ensure_dirs() {
    let dirs = [
        paths::graphmind_dir(),
        paths::graphs_dir(),
        paths::memory_dir(),
        paths::cross_links_dir(),
        paths::sessions_dir(),
    ];
    for d in &dirs {
        fs::create_dir_all(d).ok();
    }
}

/// config.json exists but can't be used.
#[derive(Debug)]
pub enum ConfigError {
    Read(std::io::Error),
    Parse(serde_json::Error),
}

impl std::fmt::Display for ConfigError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ConfigError::Read(e) => write!(f, "cannot read {}: {e}", paths::config_path().display()),
            ConfigError::Parse(e) => write!(f, "{} is not valid JSON: {e}", paths::config_path().display()),
        }
    }
}

/// A config followed by stray bytes (e.g. the tail of an interrupted concurrent write) still
/// parses: the config itself is intact, and the next save rewrites the whole file.
fn parse_config(raw: &str) -> serde_json::Result<GlobalConfig> {
    serde_json::from_str(raw).or_else(|strict| {
        serde_json::Deserializer::from_str(raw)
            .into_iter::<GlobalConfig>()
            .next()
            .unwrap_or(Err(strict))
    })
}

/// The config on disk; defaults when there is no config.json yet, or an empty one: a `touch`, or
/// a crash during an older version's non-atomic write, which can also leave the file's length
/// filled with NUL bytes. Either way there is nothing in it to protect.
pub fn read_config() -> Result<GlobalConfig, ConfigError> {
    match fs::read_to_string(paths::config_path()) {
        Ok(raw) if raw.trim_matches(|c: char| c.is_whitespace() || c == '\0').is_empty() => {
            Ok(GlobalConfig::default())
        }
        Ok(raw) => parse_config(&raw).map_err(ConfigError::Parse),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(GlobalConfig::default()),
        Err(e) => Err(ConfigError::Read(e)),
    }
}

/// The config, or defaults (with a warning, once per process) when config.json can't be read.
/// For read-only use: anything that writes the config back is protected by `save_config`.
pub fn load_config() -> GlobalConfig {
    read_config().unwrap_or_else(|e| {
        static WARNED: std::sync::Once = std::sync::Once::new();
        WARNED.call_once(|| eprintln!("Warning: {e}. Using defaults; fix or remove the file."));
        GlobalConfig::default()
    })
}

/// Writes `config` atomically (temp file + rename on the same filesystem),
/// so a concurrent reader always sees either the old or the new complete
/// file, never a truncated/corrupt one. Refuses while config.json exists but
/// can't be read: `config` would then be the defaults `load_config` fell back
/// to, and saving them would unregister every project.
pub fn save_config(config: &GlobalConfig) {
    if let Err(e) = read_config() {
        eprintln!("Error: not saving the config: {e}. Fix or remove the file first.");
        return;
    }
    ensure_dirs();
    let json = serde_json::to_string_pretty(config).unwrap_or_default();
    let path = paths::config_path();
    let tmp_path = path.with_extension(format!(
        "tmp.{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
    ));
    if let Err(e) = fs::write(&tmp_path, json) {
        eprintln!("Warning: Failed to write config: {e}");
        return;
    }
    if let Err(e) = fs::rename(&tmp_path, &path) {
        eprintln!("Warning: Failed to write config: {e}");
    }
}

/// Runs `f` while holding an exclusive lock on `~/.graphmind/config.lock`,
/// serializing read-modify-write sequences against `config.json` across
/// processes (e.g. several `graphmind`/`graphmind mcp` processes each
/// registering their own worktree at the same time). Without this, two
/// concurrent writers can each read the same config, apply their own
/// change, and the second `save_config` silently clobbers the first's.
///
/// Best-effort: if the lock file can't be opened, `f` still runs
/// unlocked rather than failing the whole operation.
fn with_config_lock<R>(f: impl FnOnce() -> R) -> R {
    use fs4::FileExt;
    ensure_dirs();
    match fs::OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(false)
        .open(paths::config_lock_path())
    {
        Ok(lock_file) => {
            let _ = FileExt::lock(&lock_file);
            let result = f();
            let _ = lock_file.unlock();
            result
        }
        Err(_) => f(),
    }
}

pub fn slugify(input: &str) -> String {
    let name = Path::new(input)
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| input.to_string());
    name.to_lowercase()
        .replace([' ', '_'], "-")
        .chars()
        .filter(|c| c.is_alphanumeric() || *c == '-')
        .collect()
}

fn canonical(path: &str) -> String {
    fs::canonicalize(path)
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|_| path.to_string())
}

/// Read-modify-write of config.json under the cross-process lock. `f` returns its
/// result and whether the config changed (only then is it written). `None` when
/// the config on disk is unreadable: it is left as is.
fn mutate_config<R>(f: impl FnOnce(&mut GlobalConfig) -> (R, bool)) -> Option<R> {
    with_config_lock(|| {
        let mut config = match read_config() {
            Ok(config) => config,
            Err(e) => {
                eprintln!("Error: not modifying the config: {e}. Fix or remove the file first.");
                return None;
            }
        };
        let (result, changed) = f(&mut config);
        if changed {
            save_config(&config);
        }
        Some(result)
    })
}

/// `base`, unless a project at another path already uses it: then `base-<hash of
/// the path>`. Worktrees are often named alike (`~/a/app`, `~/b/app`), and reusing
/// the slug would silently re-point the existing project at the new path.
fn unique_slug(config: &GlobalConfig, base: &str, abs_path: &str) -> String {
    let free = |s: &str| config.projects.get(s).is_none_or(|p| p.path == abs_path);
    if free(base) {
        return base.to_string();
    }
    let hash = format!("{:x}", Sha256::digest(abs_path.as_bytes()));
    let short = format!("{base}-{}", &hash[..6]);
    if free(&short) {
        short
    } else {
        format!("{base}-{}", &hash[..12])
    }
}

/// Fills `repo_id` for projects registered before it existed. Returns whether any
/// project changed. One git call per such project, once: the result is saved.
fn backfill_repo_ids(config: &mut GlobalConfig) -> bool {
    let mut changed = false;
    for p in config.projects.values_mut() {
        if p.repo_id.is_none() && Path::new(&p.path).is_dir() {
            if let Some(id) = git_identity::repo_id(Path::new(&p.path)) {
                p.repo_id = Some(id);
                changed = true;
            }
        }
    }
    changed
}

/// The registered project of `repo_id` that `dir` corresponds to in the checkout
/// rooted at `root`: the one whose position inside its own checkout contains `dir`
/// (deepest wins, so a monorepo's `packages/api` beats the repo root). Returns
/// that project and where it sits in `root`.
fn family_match(
    config: &GlobalConfig,
    repo_id: &str,
    root: &Path,
    dir: &Path,
) -> Option<(PathBuf, ProjectConfig)> {
    config
        .projects
        .values()
        .filter(|p| p.repo_id.as_deref() == Some(repo_id) && Path::new(&p.path) != dir)
        .filter_map(|p| {
            let p_root = git_identity::work_tree(Path::new(&p.path))?.root;
            let rel = Path::new(&p.path).strip_prefix(&p_root).ok()?.to_path_buf();
            // `join("")` would append a trailing separator, and registered paths
            // are compared as strings.
            let here = if rel.as_os_str().is_empty() { root.to_path_buf() } else { root.join(&rel) };
            dir.starts_with(&here).then(|| (rel.components().count(), here, p.clone()))
        })
        .max_by_key(|(depth, _, _)| *depth)
        .map(|(_, here, p)| (here, p))
}

pub struct Registry;

impl Registry {
    pub fn register(path: &str, slug: Option<&str>, exclude: &[String]) -> ProjectConfig {
        let abs_path = canonical(path);
        let repo_id = git_identity::repo_id(Path::new(&abs_path));
        let root = git_identity::work_tree(Path::new(&abs_path)).map(|w| w.root);
        let branch = git_identity::current_branch(Path::new(&abs_path));
        let now = chrono::Utc::now().to_rfc3339();

        let build = |slug: String, primary: Option<String>| ProjectConfig {
            path: abs_path.clone(),
            slug,
            registered: now.clone(),
            last_build: None,
            auto_watch: false,
            languages: Vec::new(),
            exclude: exclude.to_vec(),
            repo_id: repo_id.clone(),
            branch: branch.clone(),
            primary,
        };

        let project = mutate_config(|config| {
            backfill_repo_ids(config);
            let existing = config.projects.values().find(|p| p.path == abs_path).cloned();
            let slug = match (slug, &existing) {
                (Some(s), _) => s.to_string(),
                // Re-running `init` keeps the project's name, auto-linked worktrees included.
                (None, Some(e)) => e.slug.clone(),
                (None, None) => unique_slug(config, &slugify(&abs_path), &abs_path),
            };
            // Another worktree of a registered repo shares that project's memory.
            let primary = match &existing {
                Some(e) => e.primary.clone(),
                None => match (&repo_id, &root) {
                    (Some(id), Some(root)) => family_match(config, id, root, Path::new(&abs_path))
                        .filter(|(here, _)| here == Path::new(&abs_path))
                        .map(|(_, p)| p.primary.unwrap_or(p.slug)),
                    _ => None,
                },
            };
            let project = build(slug.clone(), primary);
            config.projects.insert(slug, project.clone());
            (project, true)
        })
        .unwrap_or_else(|| build(slug.map(String::from).unwrap_or_else(|| slugify(&abs_path)), None));

        fs::create_dir_all(paths::graph_dir(&project.slug)).ok();

        project
    }

    /// Registers the checkout containing `dir` when it is another worktree of a
    /// registered repo, at the same place in the checkout as the registered
    /// project (`git worktree add`, Claude Code's `.claude/worktrees/<name>`, …).
    /// The new project shares that project's memory (`primary`) and excludes, and
    /// gets its own graph, seeded from that project's (see [`crate::seed`]). Also forgets auto-linked worktrees whose directory is
    /// gone: their memory lives with the primary, so nothing is lost.
    ///
    /// `None` when `dir` isn't in a git repo or no registered project matches.
    pub fn link_worktree(dir: &Path) -> Option<ProjectConfig> {
        let repo_id = git_identity::repo_id(dir)?;
        // Lock-free early exit for the common case — a repo graphmind doesn't know —
        // since hooks run this on every tool call there.
        let known = load_config().projects.values().any(|p| {
            p.repo_id.as_deref() == Some(repo_id.as_str())
                || (p.repo_id.is_none() && Path::new(&p.path).is_dir())
        });
        if !known {
            return None;
        }
        let root = git_identity::work_tree(dir)?.root;
        let dir = dir.canonicalize().unwrap_or_else(|_| dir.to_path_buf());

        let (linked, pruned) = mutate_config(|config| {
            let backfilled = backfill_repo_ids(config);
            let Some((here, sibling)) = family_match(config, &repo_id, &root, &dir) else {
                return ((None, Vec::new()), backfilled);
            };
            let path = here.to_string_lossy().to_string();
            let slug = unique_slug(config, &slugify(&path), &path);
            let project = ProjectConfig {
                path,
                slug: slug.clone(),
                registered: chrono::Utc::now().to_rfc3339(),
                last_build: None,
                auto_watch: false,
                languages: Vec::new(),
                exclude: sibling.exclude.clone(),
                repo_id: Some(repo_id.clone()),
                branch: git_identity::current_branch(&here),
                primary: Some(sibling.primary.clone().unwrap_or(sibling.slug.clone())),
            };
            config.projects.insert(slug.clone(), project.clone());
            // Locked before the registration is visible to other processes, so a
            // build they start waits for the seed below instead of racing it. Never
            // waited for while holding the config lock (a build takes them in the
            // other order): if it is taken, there is no seed, just a full build.
            let lock = crate::seed::try_lock_build(&slug);

            let gone: Vec<String> = config
                .projects
                .values()
                .filter(|p| p.primary.is_some() && !Path::new(&p.path).exists())
                .map(|p| p.slug.clone())
                .collect();
            for slug in &gone {
                config.projects.remove(slug);
            }
            ((Some((project, sibling.slug, lock)), gone), true)
        })?;

        for slug in pruned {
            fs::remove_dir_all(paths::graph_dir(&slug)).ok();
        }
        let (project, sibling, lock) = linked?;
        if lock.is_some() {
            crate::seed::seed_graph(&project.slug, &sibling);
        }
        drop(lock);
        crate::seed::worktree_linked(&project);
        Some(project)
    }

    /// Fills in `repo_id` for every project registered before it existed, so their
    /// worktrees can be recognized. Cheap after the first run.
    pub fn backfill_repo_ids() {
        let needed = load_config()
            .projects
            .values()
            .any(|p| p.repo_id.is_none() && Path::new(&p.path).is_dir());
        if needed {
            mutate_config(|config| ((), backfill_repo_ids(config)));
        }
    }

    /// The key `slug`'s memory is stored under: its primary project's for a
    /// worktree linked to one (decisions and conventions are repo-wide), else
    /// `slug` itself — exactly what every project used before worktree support.
    pub fn memory_key(slug: &str) -> String {
        Self::get(slug)
            .and_then(|p| p.primary)
            .unwrap_or_else(|| slug.to_string())
    }

    pub fn unregister(slug: &str) -> bool {
        mutate_config(|config| {
            let removed = config.projects.remove(slug).is_some();
            (removed, removed)
        })
        .unwrap_or(false)
    }

    pub fn get(slug: &str) -> Option<ProjectConfig> {
        let config = load_config();
        config.projects.get(slug).cloned()
    }

    /// Registered projects, sorted by slug (stable output for list and cross-project search).
    pub fn list() -> Vec<ProjectConfig> {
        let config = load_config();
        let mut projects: Vec<_> = config.projects.values().cloned().collect();
        projects.sort_by(|a, b| a.slug.cmp(&b.slug));
        projects
    }

    /// The registered project containing `path` (deepest root wins). Not when a
    /// linked worktree starts between the project's root and `path`: Claude Code
    /// puts worktrees in `<repo>/.claude/worktrees/<name>`, and answering from the
    /// main checkout's graph there would silently describe another branch.
    pub fn find_by_path(path: &str) -> Option<ProjectConfig> {
        let config = load_config();
        let abs_path = canonical(path);
        let best = config
            .projects
            .values()
            .filter(|p| abs_path == p.path || abs_path.starts_with(&format!("{}/", p.path)))
            .max_by_key(|p| p.path.len())?;
        if best.path != abs_path {
            if let Some(wt) = git_identity::work_tree(Path::new(&abs_path)) {
                if wt.linked && wt.root != Path::new(&best.path) && wt.root.starts_with(&best.path) {
                    return None;
                }
            }
        }
        Some(best.clone())
    }

    pub fn update_project(slug: &str, f: impl FnOnce(&mut ProjectConfig)) {
        mutate_config(|config| match config.projects.get_mut(slug) {
            Some(project) => {
                f(project);
                ((), true)
            }
            None => ((), false),
        });
    }

    pub fn get_config() -> GlobalConfig {
        load_config()
    }
}

#[cfg(test)]
mod startup_config_tests {
    use super::*;

    #[test]
    fn global_config_default_startup_fields_are_false() {
        let cfg = GlobalConfig::default();
        assert!(!cfg.launch_at_login);
        assert!(!cfg.build_all_on_startup);
    }

    #[test]
    fn startup_fields_serialize_deserialize() {
        let cfg = GlobalConfig {
            launch_at_login: true,
            build_all_on_startup: true,
            ..GlobalConfig::default()
        };
        let json = serde_json::to_string(&cfg).unwrap();
        let parsed: GlobalConfig = serde_json::from_str(&json).unwrap();
        assert!(parsed.launch_at_login);
        assert!(parsed.build_all_on_startup);
    }

    #[test]
    fn missing_startup_fields_deserialize_to_false() {
        let json = r#"{"version":"1","projects":{},"global_exclude":[],"defaults":{"embedding_model":"minilm","watch_debounce":2000,"max_depth":5,"exclude_tests":true},"mcp":{"transport":"stdio","http_port":37378,"restrict_to_projects":null}}"#;
        let cfg: GlobalConfig = serde_json::from_str(json).unwrap();
        assert!(!cfg.launch_at_login);
        assert!(!cfg.build_all_on_startup);
    }
}

/// Serializes the tests that point HOME at a temp dir: HOME is process-wide.
#[cfg(test)]
static TEST_HOME_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

#[cfg(test)]
mod concurrent_registration_tests {
    use super::*;

    /// Simulates several processes each registering their own worktree at
    /// roughly the same time (e.g. multiple Claude Code agents, each
    /// running `graphmind init` in a different git worktree of the same
    /// repo). Without with_config_lock serializing the read-modify-write
    /// in Registry::register, a losing thread's save_config would clobber
    /// a winning thread's — this asserts none are lost.
    #[test]
    fn concurrent_register_calls_lose_no_project() {
        let _home = super::TEST_HOME_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let dir = tempfile::tempdir().unwrap();
        unsafe { std::env::set_var("HOME", dir.path()) };

        const N: usize = 12;
        let mut project_dirs = Vec::new();
        for i in 0..N {
            let p = dir.path().join(format!("project-{i}"));
            fs::create_dir_all(&p).unwrap();
            project_dirs.push(p);
        }

        let handles: Vec<_> = project_dirs
            .into_iter()
            .enumerate()
            .map(|(i, path)| {
                std::thread::spawn(move || {
                    Registry::register(path.to_str().unwrap(), Some(&format!("proj-{i}")), &[]);
                })
            })
            .collect();
        for h in handles {
            h.join().unwrap();
        }

        let registered = Registry::list();
        assert_eq!(
            registered.len(),
            N,
            "expected all {N} concurrent registrations to survive, found {}: {:?}",
            registered.len(),
            registered.iter().map(|p| &p.slug).collect::<Vec<_>>()
        );
        for i in 0..N {
            assert!(
                Registry::get(&format!("proj-{i}")).is_some(),
                "proj-{i} should be registered"
            );
        }
    }
}

#[cfg(test)]
mod parse_config_tests {
    use super::*;

    fn valid_json() -> String {
        serde_json::to_string_pretty(&GlobalConfig::default()).unwrap()
    }

    #[test]
    fn trailing_bytes_after_a_valid_config_parse() {
        assert!(parse_config(&valid_json()).is_ok());
        assert!(parse_config(&(valid_json() + "aw")).is_ok());
    }

    /// An empty, whitespace-only or NUL-filled config.json is like a missing one: nothing to
    /// protect, so it reads as the defaults and must not block every save.
    #[test]
    fn empty_config_file_reads_as_defaults() {
        let _home = super::TEST_HOME_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let dir = tempfile::tempdir().unwrap();
        unsafe { std::env::set_var("HOME", dir.path()) };
        ensure_dirs();
        let defaults = serde_json::to_value(GlobalConfig::default()).unwrap();
        for raw in ["", "  \n\t", "\0\0\0\0", "\0\0\n\0"] {
            fs::write(paths::config_path(), raw).unwrap();
            let config = read_config().unwrap_or_else(|e| panic!("{raw:?}: {e}"));
            assert_eq!(serde_json::to_value(config).unwrap(), defaults, "{raw:?}");
        }
    }

    /// NUL bytes inside a config aren't an empty file: that config is damaged, not missing.
    #[test]
    fn nul_bytes_after_a_config_do_not_read_as_empty() {
        let _home = super::TEST_HOME_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let dir = tempfile::tempdir().unwrap();
        unsafe { std::env::set_var("HOME", dir.path()) };
        ensure_dirs();
        fs::write(paths::config_path(), "\0\0{\"projects\": [").unwrap();
        assert!(read_config().is_err());
    }

    #[test]
    fn truncated_or_garbage_does_not_parse() {
        let raw = valid_json();
        assert!(parse_config(&raw[..raw.len() / 2]).is_err());
        assert!(parse_config("not json").is_err());
    }
}

use serde::{Deserialize, Serialize};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MemoryType {
    Decision,
    Pattern,
    Convention,
    Bug,
    Context,
    Session,
}

impl MemoryType {
    /// The names `--type` and the MCP tools accept, in the order error messages list them.
    pub const NAMES: [&'static str; 6] = ["decision", "pattern", "convention", "bug", "context", "session"];
}

/// An unknown name (a typo such as "decisions") is an error listing the valid types; it must not
/// silently become `context`, which expires after 30 days.
impl std::str::FromStr for MemoryType {
    type Err = String;

    fn from_str(name: &str) -> Result<Self, Self::Err> {
        match name {
            "decision" => Ok(MemoryType::Decision),
            "pattern" => Ok(MemoryType::Pattern),
            "convention" => Ok(MemoryType::Convention),
            "bug" => Ok(MemoryType::Bug),
            "context" => Ok(MemoryType::Context),
            "session" => Ok(MemoryType::Session),
            other => Err(format!("unknown memory type \"{other}\"; use one of: {}", Self::NAMES.join(", "))),
        }
    }
}

/// Fields to change in `MemoryStore::update`; `None` leaves a field as it is.
#[derive(Debug, Default)]
pub struct MemoryUpdate {
    pub content: Option<String>,
    pub tags: Option<Vec<String>>,
    pub entry_type: Option<MemoryType>,
    pub priority: Option<bool>,
}

impl MemoryUpdate {
    pub fn is_empty(&self) -> bool {
        self.content.is_none() && self.tags.is_none() && self.entry_type.is_none() && self.priority.is_none()
    }

    /// Apply the changes to `entry`. It keeps its id, `created`, session and source; `updated` is
    /// set to `now`. The expiry is kept unless the new type has a different lifetime (bug ->
    /// decision clears it, decision -> bug starts it from now). A demotion (priority -> not) also
    /// resets the recall count, so the next consolidate doesn't promote the entry straight back;
    /// `priority: false` on an entry that isn't priority keeps its count.
    pub fn apply_to(self, entry: &mut MemoryEntry, now: chrono::DateTime<chrono::Utc>) {
        if let Some(content) = self.content {
            entry.content = content;
        }
        if let Some(tags) = self.tags {
            entry.tags = tags;
        }
        if let Some(new_type) = self.entry_type {
            let new_ttl = default_ttl_for_type(&new_type);
            if new_ttl != default_ttl_for_type(&entry.entry_type) {
                entry.ttl_days = new_ttl;
                entry.expires_at = new_ttl.map(|d| (now + chrono::Duration::days(d as i64)).to_rfc3339());
            }
            entry.entry_type = new_type;
        }
        if let Some(priority) = self.priority {
            if entry.priority && !priority {
                entry.recall_count = 0;
            }
            entry.priority = priority;
        }
        entry.updated = now.to_rfc3339();
    }
}

/// Shortest id prefix accepted by lookups: what listings print. A shorter prefix that happens to
/// match one entry would otherwise show, change or delete it.
pub const MIN_ID_PREFIX: usize = 8;

#[derive(Debug, PartialEq)]
pub enum MemoryError {
    NotFound,
    /// The id prefix matches this many entries.
    Ambiguous(usize),
    /// The id prefix is shorter than `MIN_ID_PREFIX`.
    TooShort,
    /// The update has no fields to change.
    NothingToChange,
    /// The memory file couldn't be read or written; it is unchanged.
    NotWritten,
}

impl std::fmt::Display for MemoryError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MemoryError::NotFound => write!(f, "no memory entry with that id"),
            MemoryError::Ambiguous(n) => write!(f, "{n} memory entries start with that id; use more characters"),
            MemoryError::TooShort => write!(f, "use at least {MIN_ID_PREFIX} characters of the id (as listings print it)"),
            MemoryError::NothingToChange => write!(f, "nothing to change"),
            MemoryError::NotWritten => write!(f, "the memory file could not be updated; it is unchanged"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum MemorySource {
    #[default]
    Manual,
    Consolidate,
    Heuristic,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryEntry {
    pub id: String,
    pub created: String,
    pub updated: String,
    pub project: Option<String>,
    pub global: bool,
    #[serde(rename = "type")]
    pub entry_type: MemoryType,
    pub content: String,
    pub tags: Vec<String>,
    pub session: String,
    #[serde(default)]
    pub priority: bool,
    #[serde(default)]
    pub ttl_days: Option<u32>,
    #[serde(default)]
    pub recall_count: u32,
    #[serde(default = "default_confidence")]
    pub confidence: f32,
    #[serde(default)]
    pub source: MemorySource,
    #[serde(default)]
    pub expires_at: Option<String>,
}

fn default_confidence() -> f32 {
    1.0
}

pub struct AddOptions {
    pub project: Option<String>,
    pub global: bool,
    pub entry_type: MemoryType,
    pub tags: Vec<String>,
    pub priority: bool,
    pub ttl_days: Option<u32>,
    pub confidence: f32,
    pub source: MemorySource,
}

impl Default for AddOptions {
    fn default() -> Self {
        Self {
            project: None,
            global: false,
            entry_type: MemoryType::Context,
            tags: Vec::new(),
            priority: false,
            ttl_days: None,
            confidence: 1.0,
            source: MemorySource::Manual,
        }
    }
}

/// Return the default TTL in days for a given MemoryType.
pub fn default_ttl_for_type(entry_type: &MemoryType) -> Option<u32> {
    match entry_type {
        MemoryType::Decision | MemoryType::Pattern | MemoryType::Convention => None,
        MemoryType::Bug => Some(90),
        MemoryType::Context => Some(30),
        MemoryType::Session => Some(7),
    }
}

pub struct MemoryStore {
    pub memory_dir: PathBuf,
}

impl MemoryStore {
    pub fn new(memory_dir: &Path) -> Self {
        fs::create_dir_all(memory_dir).ok();
        Self {
            memory_dir: memory_dir.to_path_buf(),
        }
    }

    pub fn add(&self, content: &str, options: AddOptions) -> MemoryEntry {
        let now = chrono::Utc::now().to_rfc3339();

        // Compute expires_at from ttl_days if provided
        let expires_at = options.ttl_days.map(|days| {
            let dur = chrono::Duration::days(days as i64);
            (chrono::Utc::now() + dur).to_rfc3339()
        });

        let entry = MemoryEntry {
            id: Uuid::new_v4().to_string(),
            created: now.clone(),
            updated: now.clone(),
            project: options.project.clone(),
            global: options.global,
            entry_type: options.entry_type,
            content: content.to_string(),
            tags: options.tags,
            session: now[..10].to_string(),
            priority: options.priority,
            ttl_days: options.ttl_days,
            recall_count: 0,
            confidence: options.confidence,
            source: options.source,
            expires_at,
        };

        let file_path = if entry.global {
            self.global_path()
        } else if let Some(ref proj) = entry.project {
            self.project_path(proj)
        } else {
            self.global_path()
        };

        // Under the lock: an append between another process's read and its rename (recall
        // count, delete, consolidate) would otherwise be overwritten by that rewrite.
        let line = serde_json::to_string(&entry).unwrap_or_default();
        self.with_file_lock(&file_path, || self.atomic_append(&file_path, &line));
        entry
    }

    pub fn list(&self, project: Option<&str>) -> Vec<MemoryEntry> {
        let mut entries = Vec::new();

        let global_path = self.global_path();
        if global_path.exists() {
            entries.extend(self.read_jsonl(&global_path));
        }

        if let Some(proj) = project {
            let proj_path = self.project_path(proj);
            if proj_path.exists() {
                entries.extend(self.read_jsonl(&proj_path));
            }
        }

        entries.sort_by(|a, b| b.created.cmp(&a.created));
        entries
    }

    pub fn list_priority(&self, project: Option<&str>) -> Vec<MemoryEntry> {
        self.list(project)
            .into_iter()
            .filter(|e| e.priority)
            .collect()
    }

    /// The memory files `project` reads: the global file, then the project's.
    fn files(&self, project: Option<&str>) -> Vec<PathBuf> {
        let mut paths = vec![self.global_path()];
        if let Some(proj) = project {
            paths.push(self.project_path(proj));
        }
        paths
    }

    /// The memory file and full id of the one entry whose id is `id` or starts with it (listings
    /// print 8-character prefixes; shorter ones are refused). An exact id always wins over longer
    /// ids sharing it as a prefix. The one lookup for show, update and delete.
    pub fn find(&self, id: &str, project: Option<&str>) -> Result<(PathBuf, String), MemoryError> {
        if id.chars().count() < MIN_ID_PREFIX {
            return Err(MemoryError::TooShort);
        }
        let matches: Vec<(PathBuf, String)> = self
            .files(project)
            .into_iter()
            .filter(|p| p.exists())
            .flat_map(|p| {
                self.read_jsonl(&p)
                    .into_iter()
                    .filter(|e| e.id.starts_with(id))
                    .map(|e| (p.clone(), e.id))
                    .collect::<Vec<_>>()
            })
            .collect();
        if let Some(exact) = matches.iter().find(|(_, full)| full == id) {
            return Ok(exact.clone());
        }
        match matches.as_slice() {
            [] => Err(MemoryError::NotFound),
            [one] => Ok(one.clone()),
            many => Err(MemoryError::Ambiguous(many.len())),
        }
    }

    /// The entry `find` resolves `id` to.
    pub fn get(&self, id: &str, project: Option<&str>) -> Result<MemoryEntry, MemoryError> {
        let (path, full_id) = self.find(id, project)?;
        self.read_jsonl(&path).into_iter().find(|e| e.id == full_id).ok_or(MemoryError::NotFound)
    }

    /// Edit an entry in place (see `MemoryUpdate::apply_to`). `id` may be the 8-character prefix
    /// that listings print.
    pub fn update(&self, id: &str, project: Option<&str>, edit: MemoryUpdate) -> Result<MemoryEntry, MemoryError> {
        if edit.is_empty() {
            return Err(MemoryError::NothingToChange);
        }
        let (path, full_id) = self.find(id, project)?;
        self.update_file(&path, |entries| match entries.iter_mut().find(|e| e.id == full_id) {
            Some(entry) => {
                edit.apply_to(entry, chrono::Utc::now());
                (Ok(entry.clone()), true)
            }
            None => (Err(MemoryError::NotFound), false),
        })
        .unwrap_or(Err(MemoryError::NotWritten))
    }

    /// Delete one entry by id or id prefix; returns the full id deleted.
    pub fn delete_entry(&self, id: &str, project: Option<&str>) -> Result<String, MemoryError> {
        let (path, full_id) = self.find(id, project)?;
        self.update_file(&path, |entries| {
            let before = entries.len();
            entries.retain(|e| e.id != full_id);
            if entries.len() == before {
                (Err(MemoryError::NotFound), false)
            } else {
                (Ok(full_id.clone()), true)
            }
        })
        .unwrap_or(Err(MemoryError::NotWritten))
    }

    /// Increment the recall_count for the entry with the given id.
    pub fn increment_recall(&self, id: &str, project: Option<&str>) {
        for path in self.files(project).iter().filter(|p| p.exists()) {
            let found = self.update_file(path, |entries| match entries.iter_mut().find(|e| e.id == id) {
                Some(entry) => {
                    entry.recall_count += 1;
                    (true, true)
                }
                None => (false, false),
            });
            if found == Some(true) {
                return;
            }
        }
    }

    /// Read-modify-write of one memory file under its lock. `f` gets the parsed entries and
    /// returns (result, changed); the file is rewritten only when it changed, with the lines this
    /// version can't parse kept after the entries. `None` means the file is unchanged because it
    /// exists but can't be read (then `f` doesn't run: its entries would read as none, and a
    /// rewrite would empty it) or because the write failed. `f` runs under the file's lock, so it
    /// must not call back into the store for the same file: the second flock would deadlock.
    pub fn update_file<R>(&self, path: &Path, f: impl FnOnce(&mut Vec<MemoryEntry>) -> (R, bool)) -> Option<R> {
        self.with_file_lock(path, || {
            let raw = match fs::read_to_string(path) {
                Ok(raw) => raw,
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
                Err(e) => {
                    eprintln!("Warning: memory file {} not updated: it can't be read ({e})", path.display());
                    return None;
                }
            };
            let mut entries = Vec::new();
            let mut unparsed = Vec::new();
            for line in raw.lines().filter(|l| !l.trim().is_empty()) {
                match serde_json::from_str::<MemoryEntry>(line) {
                    Ok(entry) => entries.push(entry),
                    Err(_) => unparsed.push(line),
                }
            }
            let (result, changed) = f(&mut entries);
            if changed {
                let mut content = String::new();
                for entry in &entries {
                    content.push_str(&serde_json::to_string(entry).unwrap_or_default());
                    content.push('\n');
                }
                for line in unparsed {
                    content.push_str(line);
                    content.push('\n');
                }
                if let Err(e) = self.atomic_write(path, &content) {
                    eprintln!("Warning: memory file {} not updated: {e}", path.display());
                    return None;
                }
            }
            Some(result)
        })
    }

    pub fn global_path(&self) -> PathBuf {
        self.memory_dir.join("global.jsonl")
    }

    pub fn project_path(&self, slug: &str) -> PathBuf {
        self.memory_dir.join(format!("{slug}.jsonl"))
    }

    fn read_jsonl(&self, file_path: &Path) -> Vec<MemoryEntry> {
        let content = match fs::read_to_string(file_path) {
            Ok(c) => c,
            Err(_) => return Vec::new(),
        };
        content
            .lines()
            .filter(|l| !l.trim().is_empty())
            .filter_map(|l| serde_json::from_str(l).ok())
            .collect()
    }

    fn atomic_append(&self, file_path: &Path, line: &str) {
        if let Some(parent) = file_path.parent() {
            fs::create_dir_all(parent).ok();
        }
        let mut file = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(file_path)
            .expect("Failed to open memory file");
        writeln!(file, "{line}").ok();
    }

    /// Runs `f` while holding an exclusive lock on `<file_path>.lock`,
    /// serializing read-modify-write sequences against a given JSONL file
    /// across processes — e.g. `increment_recall` from one MCP server and
    /// `delete`/`consolidate` from another CLI invocation, both targeting
    /// the same shared repo_id-keyed memory file. `add` takes it too: an
    /// append is atomic on its own, but one that lands between another
    /// process's read and its rename is lost with that rewrite.
    ///
    /// Best-effort: if the lock file can't be opened, `f` still runs
    /// unlocked rather than failing the whole operation.
    fn with_file_lock<R>(&self, file_path: &Path, f: impl FnOnce() -> R) -> R {
        use fs4::FileExt;
        if let Some(parent) = file_path.parent() {
            fs::create_dir_all(parent).ok();
        }
        let lock_path = file_path.with_extension(match file_path.extension() {
            Some(ext) => format!("{}.lock", ext.to_string_lossy()),
            None => "lock".to_string(),
        });
        match fs::OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(false)
            .open(&lock_path)
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

    /// Replace the file with `content`: written and synced to a temp file first, renamed over the
    /// original only if that succeeded, so a full disk or I/O error leaves the old file intact.
    fn atomic_write(&self, file_path: &Path, content: &str) -> std::io::Result<()> {
        let tmp_path = file_path.with_extension(format!(
            "tmp.{}.{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        ));
        let result = fs::File::create(&tmp_path)
            .and_then(|mut f| {
                f.write_all(content.as_bytes())?;
                f.sync_all()
            })
            .and_then(|_| fs::rename(&tmp_path, file_path));
        if result.is_err() {
            fs::remove_file(&tmp_path).ok();
        }
        result
    }
}

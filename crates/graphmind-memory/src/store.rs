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

        self.atomic_append(&file_path, &serde_json::to_string(&entry).unwrap_or_default());
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

    pub fn delete(&self, id: &str, project: Option<&str>) -> bool {
        let mut paths = vec![self.global_path()];
        if let Some(proj) = project {
            paths.push(self.project_path(proj));
        }

        for file_path in &paths {
            if !file_path.exists() {
                continue;
            }
            let deleted = self.with_file_lock(file_path, || {
                let entries = self.read_jsonl(file_path);
                let filtered: Vec<_> = entries.iter().filter(|e| e.id != id).collect();
                if filtered.len() != entries.len() {
                    let content = if filtered.is_empty() {
                        String::new()
                    } else {
                        filtered
                            .iter()
                            .map(|e| serde_json::to_string(e).unwrap_or_default())
                            .collect::<Vec<_>>()
                            .join("\n")
                            + "\n"
                    };
                    self.atomic_write(file_path, &content);
                    true
                } else {
                    false
                }
            });
            if deleted {
                return true;
            }
        }
        false
    }

    /// Increment the recall_count for the entry with the given id.
    /// Rewrites the JSONL file with the updated count.
    pub fn increment_recall(&self, id: &str, project: Option<&str>) {
        let mut paths = vec![self.global_path()];
        if let Some(proj) = project {
            paths.push(self.project_path(proj));
        }

        for file_path in &paths {
            if !file_path.exists() {
                continue;
            }
            let found = self.with_file_lock(file_path, || {
                let mut entries = self.read_jsonl(file_path);
                let mut found = false;
                for entry in &mut entries {
                    if entry.id == id {
                        entry.recall_count += 1;
                        found = true;
                        break;
                    }
                }
                if found {
                    let content = if entries.is_empty() {
                        String::new()
                    } else {
                        entries
                            .iter()
                            .map(|e| serde_json::to_string(e).unwrap_or_default())
                            .collect::<Vec<_>>()
                            .join("\n")
                            + "\n"
                    };
                    self.atomic_write(file_path, &content);
                }
                found
            });
            if found {
                return;
            }
        }
    }

    /// Rewrite a JSONL file with the given entries (used by consolidate).
    pub fn rewrite_file(&self, file_path: &Path, entries: &[MemoryEntry]) {
        let content = if entries.is_empty() {
            String::new()
        } else {
            entries
                .iter()
                .map(|e| serde_json::to_string(e).unwrap_or_default())
                .collect::<Vec<_>>()
                .join("\n")
                + "\n"
        };
        self.atomic_write(file_path, &content);
    }

    pub fn global_path(&self) -> PathBuf {
        self.memory_dir.join("global.jsonl")
    }

    pub fn project_path(&self, slug: &str) -> PathBuf {
        self.memory_dir.join(format!("{slug}.jsonl"))
    }

    pub fn read_jsonl(&self, file_path: &Path) -> Vec<MemoryEntry> {
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
    /// the same shared repo_id-keyed memory file. `atomic_append` (plain
    /// `add`) doesn't need this — an OS-level append is already atomic
    /// for lines under `PIPE_BUF`, and doesn't read the file first.
    ///
    /// Best-effort: if the lock file can't be opened, `f` still runs
    /// unlocked rather than failing the whole operation.
    pub fn with_file_lock<R>(&self, file_path: &Path, f: impl FnOnce() -> R) -> R {
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

    fn atomic_write(&self, file_path: &Path, content: &str) {
        let tmp_path = file_path.with_extension(format!(
            "tmp.{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis()
        ));
        fs::write(&tmp_path, content).ok();
        fs::rename(&tmp_path, file_path).ok();
    }
}

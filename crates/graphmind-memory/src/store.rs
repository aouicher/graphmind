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
    #[serde(default = "default_is_shared")]
    pub is_shared: bool,
    #[serde(default)]
    pub remote_id: Option<String>,
    #[serde(default)]
    pub synced_at: Option<u64>,
}

fn default_is_shared() -> bool {
    true
}

pub struct AddOptions {
    pub project: Option<String>,
    pub global: bool,
    pub entry_type: MemoryType,
    pub tags: Vec<String>,
    pub priority: bool,
}

impl Default for AddOptions {
    fn default() -> Self {
        Self {
            project: None,
            global: false,
            entry_type: MemoryType::Context,
            tags: Vec::new(),
            priority: false,
        }
    }
}

pub struct MemoryStore {
    memory_dir: PathBuf,
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
            is_shared: true,
            remote_id: None,
            synced_at: None,
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
                return true;
            }
        }
        false
    }

    fn global_path(&self) -> PathBuf {
        self.memory_dir.join("global.jsonl")
    }

    fn project_path(&self, slug: &str) -> PathBuf {
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

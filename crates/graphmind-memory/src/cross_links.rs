use serde::{Deserialize, Serialize};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum LinkType {
    DependsOn,
    SharesPattern,
    Extends,
    UsesTypesFrom,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CrossLink {
    pub id: String,
    pub from: String,
    pub to: String,
    #[serde(rename = "type")]
    pub link_type: LinkType,
    pub reason: String,
    pub symbols: Vec<String>,
    pub inferred: bool,
    pub confidence: f64,
    pub created: String,
}

pub struct NewCrossLink {
    pub from: String,
    pub to: String,
    pub link_type: LinkType,
    pub reason: String,
    pub symbols: Vec<String>,
    pub inferred: bool,
    pub confidence: f64,
}

pub struct CrossLinkStore {
    file_path: PathBuf,
}

impl CrossLinkStore {
    pub fn new(file_path: &Path) -> Self {
        Self {
            file_path: file_path.to_path_buf(),
        }
    }

    pub fn add(&self, link: NewCrossLink) -> CrossLink {
        let entry = CrossLink {
            id: Uuid::new_v4().to_string(),
            from: link.from,
            to: link.to,
            link_type: link.link_type,
            reason: link.reason,
            symbols: link.symbols,
            inferred: link.inferred,
            confidence: link.confidence,
            created: chrono::Utc::now().to_rfc3339(),
        };

        if let Some(parent) = self.file_path.parent() {
            fs::create_dir_all(parent).ok();
        }
        let mut file = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.file_path)
            .expect("Failed to open cross-links file");
        writeln!(file, "{}", serde_json::to_string(&entry).unwrap_or_default()).ok();
        entry
    }

    pub fn list(&self) -> Vec<CrossLink> {
        if !self.file_path.exists() {
            return Vec::new();
        }
        let content = match fs::read_to_string(&self.file_path) {
            Ok(c) => c,
            Err(_) => return Vec::new(),
        };
        content
            .lines()
            .filter(|l| !l.trim().is_empty())
            .filter_map(|l| serde_json::from_str(l).ok())
            .collect()
    }

    pub fn find_by_project(&self, slug: &str) -> Vec<CrossLink> {
        self.list()
            .into_iter()
            .filter(|l| l.from == slug || l.to == slug)
            .collect()
    }

    pub fn delete(&self, id: &str) -> bool {
        if !self.file_path.exists() {
            return false;
        }
        let entries = self.list();
        let filtered: Vec<_> = entries.iter().filter(|e| e.id != id).collect();
        if filtered.len() == entries.len() {
            return false;
        }
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
        let tmp_path = self.file_path.with_extension(format!(
            "tmp.{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis()
        ));
        fs::write(&tmp_path, content).ok();
        fs::rename(&tmp_path, &self.file_path).ok();
        true
    }
}

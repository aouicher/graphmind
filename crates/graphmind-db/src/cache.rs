use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

pub struct FileHashCache {
    cache: HashMap<String, String>,
    cache_dir: PathBuf,
    cache_file: PathBuf,
}

impl FileHashCache {
    pub fn new(cache_dir: &Path) -> Self {
        let cache_file = cache_dir.join("hashes.json");
        let mut fhc = Self {
            cache: HashMap::new(),
            cache_dir: cache_dir.to_path_buf(),
            cache_file,
        };
        fhc.load();
        fhc
    }

    pub fn has_changed(&self, file_path: &str, content: &str) -> bool {
        let hash = Self::hash(content);
        self.cache.get(file_path) != Some(&hash)
    }

    pub fn update(&mut self, file_path: &str, content: &str) {
        self.cache.insert(file_path.to_string(), Self::hash(content));
    }

    pub fn remove(&mut self, file_path: &str) {
        self.cache.remove(file_path);
    }

    pub fn known_files(&self) -> Vec<String> {
        self.cache.keys().cloned().collect()
    }

    pub fn save(&self) {
        fs::create_dir_all(&self.cache_dir).ok();
        let json = serde_json::to_string(&self.cache).unwrap_or_default();
        fs::write(&self.cache_file, json).ok();
    }

    fn load(&mut self) {
        if !self.cache_file.exists() {
            return;
        }
        if let Ok(raw) = fs::read_to_string(&self.cache_file) {
            if let Ok(map) = serde_json::from_str::<HashMap<String, String>>(&raw) {
                self.cache = map;
            }
        }
    }

    fn hash(content: &str) -> String {
        let mut hasher = Sha256::new();
        hasher.update(content.as_bytes());
        format!("{:x}", hasher.finalize())
    }
}

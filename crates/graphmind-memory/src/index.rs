use crate::store::MemoryEntry;
use std::collections::{HashMap, HashSet};

pub struct MemoryIndex {
    by_tag: HashMap<String, HashSet<String>>,
    by_project: HashMap<String, HashSet<String>>,
    entries: HashMap<String, MemoryEntry>,
}

impl MemoryIndex {
    pub fn new() -> Self {
        Self {
            by_tag: HashMap::new(),
            by_project: HashMap::new(),
            entries: HashMap::new(),
        }
    }

    pub fn build(&mut self, entries: &[MemoryEntry]) {
        self.by_tag.clear();
        self.by_project.clear();
        self.entries.clear();

        for entry in entries {
            self.entries.insert(entry.id.clone(), entry.clone());

            for tag in &entry.tags {
                let tag_lower = tag.to_lowercase();
                self.by_tag
                    .entry(tag_lower)
                    .or_default()
                    .insert(entry.id.clone());
            }

            let proj = entry
                .project
                .as_deref()
                .unwrap_or("__global__")
                .to_string();
            self.by_project
                .entry(proj)
                .or_default()
                .insert(entry.id.clone());
        }
    }

    pub fn find_by_tag(&self, tag: &str) -> Vec<MemoryEntry> {
        let ids = match self.by_tag.get(&tag.to_lowercase()) {
            Some(ids) => ids,
            None => return Vec::new(),
        };
        ids.iter()
            .filter_map(|id| self.entries.get(id).cloned())
            .collect()
    }

    pub fn find_by_project(&self, project: &str) -> Vec<MemoryEntry> {
        let ids = match self.by_project.get(project) {
            Some(ids) => ids,
            None => return Vec::new(),
        };
        ids.iter()
            .filter_map(|id| self.entries.get(id).cloned())
            .collect()
    }

    pub fn get(&self, id: &str) -> Option<&MemoryEntry> {
        self.entries.get(id)
    }
}

impl Default for MemoryIndex {
    fn default() -> Self {
        Self::new()
    }
}

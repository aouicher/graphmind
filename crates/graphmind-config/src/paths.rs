use std::path::PathBuf;

fn home_dir() -> PathBuf {
    dirs::home_dir().expect("Cannot determine home directory")
}

pub fn graphmind_dir() -> PathBuf {
    home_dir().join(".graphmind")
}

pub fn config_path() -> PathBuf {
    graphmind_dir().join("config.json")
}

pub fn memory_dir() -> PathBuf {
    graphmind_dir().join("memory")
}

pub fn graphs_dir() -> PathBuf {
    graphmind_dir().join("graphs")
}

pub fn cross_links_dir() -> PathBuf {
    graphmind_dir().join("cross-links")
}

pub fn sessions_dir() -> PathBuf {
    graphmind_dir().join("sessions")
}

pub fn graph_dir(slug: &str) -> PathBuf {
    graphs_dir().join(slug)
}

pub fn graph_db_path(slug: &str) -> PathBuf {
    graph_dir(slug).join("graph.db")
}

pub fn cache_dir_path(slug: &str) -> PathBuf {
    graph_dir(slug).join("cache")
}

pub fn memory_path(slug: &str) -> PathBuf {
    memory_dir().join(format!("{slug}.jsonl"))
}

pub fn global_memory_path() -> PathBuf {
    memory_dir().join("global.jsonl")
}

pub fn cross_links_path() -> PathBuf {
    cross_links_dir().join("links.jsonl")
}

pub fn meta_path(slug: &str) -> PathBuf {
    graph_dir(slug).join("meta.json")
}

pub fn embedding_db_path(slug: &str) -> PathBuf {
    graph_dir(slug).join("embeddings.db")
}

pub fn memory_embedding_db_path() -> PathBuf {
    memory_dir().join("memory_embeddings.db")
}

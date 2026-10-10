//! Shared setup for the MCP handler suites: a graph of `tests/fixtures/sample-project`, built
//! once per test binary, copied into a fresh HOME for each test.

use graphmind_mcp::handlers::dispatch_tool;
use serde_json::{json, Value};
use std::path::PathBuf;
use std::sync::{Arc, Mutex, OnceLock};

// Tests must run serially because they override the HOME env var.
// Use `--test-threads=1` or acquire this lock in each test.
static TEST_LOCK: Mutex<()> = Mutex::new(());

// ── Shared graph — built once, HOME copied per test ───────────────────────────

struct SharedGraph {
    graphmind_dir: PathBuf,
    slug: String,
    _dir: Arc<tempfile::TempDir>,
}

static SHARED_GRAPH: OnceLock<SharedGraph> = OnceLock::new();

fn shared_graph() -> &'static SharedGraph {
    SHARED_GRAPH.get_or_init(|| {
        let dir = tempfile::tempdir().unwrap();
        let slug = "test-fixture".to_string();

        let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap() // crates/
            .parent()
            .unwrap() // workspace root
            .join("tests/fixtures/sample-project");

        let graphmind_dir = dir.path().join(".graphmind");
        let graph_dir = graphmind_dir.join("graphs").join(&slug);
        std::fs::create_dir_all(&graph_dir).unwrap();
        std::fs::create_dir_all(graphmind_dir.join("memory")).unwrap();

        // Minimal config.json
        let fixture_path_str = fixture.to_str().unwrap().to_string();
        let config = json!({
            "version": "1",
            "projects": {
                &slug: {
                    "slug": &slug,
                    "path": &fixture_path_str,
                    "last_build": null,
                    "languages": [],
                    "registered": "2024-01-01T00:00:00Z",
                    "auto_watch": false,
                    "exclude": []
                }
            },
            "global_exclude": [],
            "defaults": {
                "embedding_model": "minilm",
                "watch_debounce": 2000,
                "max_depth": 5,
                "exclude_tests": true
            },
            "mcp": {
                "transport": "stdio",
                "http_port": 37378,
                "restrict_to_projects": null
            }
        });
        std::fs::write(graphmind_dir.join("config.json"), config.to_string()).unwrap();

        // Build the graph once into the shared dir
        let db_path = graph_dir.join("graph.db");
        let cache_dir = graph_dir.join("cache");
        std::fs::create_dir_all(&cache_dir).unwrap();
        let mut builder = graphmind_db::builder::GraphBuilder::new(
            db_path.to_str().unwrap(),
            cache_dir.to_str().unwrap(),
        );
        builder.build(
            fixture.to_str().unwrap(),
            &graphmind_db::builder::BuildOptions {
                full: true,
                ..Default::default()
            },
        );

        SharedGraph {
            graphmind_dir,
            slug,
            _dir: Arc::new(dir),
        }
    })
}

pub struct TestCtx {
    pub slug: String,
    /// The test's HOME; `.graphmind/` inside it holds the config, graphs and memory.
    pub _dir: tempfile::TempDir,
    _lock: std::sync::MutexGuard<'static, ()>,
}

pub fn setup() -> TestCtx {
    let lock = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let shared = shared_graph();

    // Each test gets its own HOME with the shared graph copied in.
    // Memory tests mutate the JSONL store, so isolation is required.
    let dir = tempfile::tempdir().unwrap();
    let dest_graphmind = dir.path().join(".graphmind");
    copy_dir_all(&shared.graphmind_dir, &dest_graphmind).unwrap();

    // Rewrite config.json: replace shared home path with this test's home path
    let config_src = shared.graphmind_dir.join("config.json");
    let config_str = std::fs::read_to_string(&config_src).unwrap();
    let shared_home = shared.graphmind_dir.parent().unwrap().to_str().unwrap();
    let new_home = dir.path().to_str().unwrap();
    let new_config = config_str.replace(shared_home, new_home);
    std::fs::write(dest_graphmind.join("config.json"), &new_config).unwrap();

    // Override HOME so graphmind_config::paths use the temp dir
    unsafe { std::env::set_var("HOME", dir.path()); }

    TestCtx {
        slug: shared.slug.clone(),
        _dir: dir,
        _lock: lock,
    }
}

fn copy_dir_all(src: &PathBuf, dst: &PathBuf) -> std::io::Result<()> {
    std::fs::create_dir_all(dst)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let ty = entry.file_type()?;
        let dest = dst.join(entry.file_name());
        if ty.is_dir() {
            copy_dir_all(&entry.path(), &dest)?;
        } else {
            std::fs::copy(entry.path(), dest)?;
        }
    }
    Ok(())
}

impl TestCtx {
    pub fn dispatch(&self, tool: &str, args: Value) -> Value {
        dispatch_tool(tool, &args)
    }

    pub fn is_error(&self, v: &Value) -> bool {
        v.get("isError").and_then(|v| v.as_bool()).unwrap_or(false)
    }

    pub fn text(&self, v: &Value) -> String {
        v["content"][0]["text"].as_str().unwrap_or("").to_string()
    }
}

//! MCP tools stay inside registered projects: memory tools accept only a registered project
//! (by slug or checkout path) or none, and `gm_file` serves only regular files inside the project.

use graphmind_mcp::handlers::dispatch_tool;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard};

// Tests override HOME, so they run one at a time.
static TEST_LOCK: Mutex<()> = Mutex::new(());

const SLUG: &str = "registered-proj";

struct Home {
    dir: tempfile::TempDir,
    project: PathBuf,
    _lock: MutexGuard<'static, ()>,
}

impl Home {
    fn memory_dir(&self) -> PathBuf {
        self.dir.path().join(".graphmind/memory")
    }

    /// Every file under the temp HOME, relative to it.
    fn files(&self) -> Vec<String> {
        fn walk(dir: &Path, root: &Path, out: &mut Vec<String>) {
            for entry in std::fs::read_dir(dir).unwrap().flatten() {
                let path = entry.path();
                if path.is_dir() {
                    walk(&path, root, out);
                } else {
                    out.push(path.strip_prefix(root).unwrap().to_string_lossy().to_string());
                }
            }
        }
        let mut out = Vec::new();
        walk(self.dir.path(), self.dir.path(), &mut out);
        out.sort();
        out
    }
}

fn setup() -> Home {
    let lock = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let dir = tempfile::tempdir().unwrap();
    let project = dir.path().join("checkout");
    std::fs::create_dir_all(&project).unwrap();
    // Registered paths are canonical (`graphmind init` stores them so).
    let project = std::fs::canonicalize(&project).unwrap();
    let graphmind_dir = dir.path().join(".graphmind");
    std::fs::create_dir_all(graphmind_dir.join("memory")).unwrap();
    let config = json!({
        "version": "1",
        "projects": {
            SLUG: {
                "slug": SLUG,
                "path": project.to_string_lossy(),
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
    unsafe { std::env::set_var("HOME", dir.path()) };
    Home { dir, project, _lock: lock }
}

fn is_error(v: &Value) -> bool {
    v.get("isError").and_then(Value::as_bool).unwrap_or(false)
}

fn text(v: &Value) -> String {
    v["content"][0]["text"].as_str().unwrap_or("").to_string()
}

#[test]
fn memory_add_accepts_only_registered_projects() {
    let home = setup();
    let absolute = home.dir.path().join("elsewhere/target").to_string_lossy().to_string();
    let before = home.files();

    for project in [absolute.as_str(), "../../escaped", "not-registered"] {
        let resp = dispatch_tool("gm_memory_add", &json!({ "content": "a fact", "project": project }));
        assert!(is_error(&resp), "{project:?} accepted: {}", text(&resp));
        assert!(text(&resp).contains(SLUG), "the error lists registered projects: {}", text(&resp));
    }

    assert_eq!(home.files(), before, "no file was created under HOME");
    assert!(!Path::new(&format!("{absolute}.jsonl")).exists());
    assert!(!home.memory_dir().join("../../escaped.jsonl").exists());

    // A registered slug, its checkout path and global memory still work.
    let resp = dispatch_tool("gm_memory_add", &json!({ "content": "by slug", "project": SLUG }));
    assert!(!is_error(&resp), "{}", text(&resp));
    let path = home.project.to_string_lossy().to_string();
    let resp = dispatch_tool("gm_memory_add", &json!({ "content": "by path", "project": path }));
    assert!(!is_error(&resp), "{}", text(&resp));
    let resp = dispatch_tool("gm_memory_add", &json!({ "content": "everywhere", "global": true }));
    assert!(!is_error(&resp), "{}", text(&resp));

    let scoped = std::fs::read_to_string(home.memory_dir().join(format!("{SLUG}.jsonl"))).unwrap();
    assert!(scoped.contains("by slug") && scoped.contains("by path"), "{scoped}");
    let global = std::fs::read_to_string(home.memory_dir().join("global.jsonl")).unwrap();
    assert!(global.contains("everywhere"), "{global}");

    let resp = dispatch_tool("gm_memory_list", &json!({ "project": SLUG }));
    assert!(!is_error(&resp), "{}", text(&resp));
    let listed = text(&resp);
    assert!(listed.contains("by slug") && listed.contains("everywhere"), "{listed}");
}

#[test]
fn other_memory_tools_refuse_a_project_that_is_not_registered() {
    let home = setup();
    let before = home.files();
    let absolute = home.dir.path().join("escaped").to_string_lossy().to_string();
    let calls = [
        ("gm_memory_search", json!({ "query": "fact", "project": "../../escaped" })),
        ("gm_memory_list", json!({ "project": absolute })),
        ("gm_memory_update", json!({ "id": "00000000", "content": "x", "project": "../escaped" })),
        ("gm_session_analyze", json!({ "facts": [{ "content": "a fact" }], "project": absolute })),
    ];
    for (tool, args) in calls {
        let resp = dispatch_tool(tool, &args);
        assert!(is_error(&resp), "{tool} accepted {args}: {}", text(&resp));
    }
    assert_eq!(home.files(), before);
}

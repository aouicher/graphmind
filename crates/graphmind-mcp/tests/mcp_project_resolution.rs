//! The `project` argument of every tool: a registered slug or the path of a registered
//! checkout resolves; anything else is an error that names it and lists the registered projects.

use graphmind_mcp::handlers::dispatch_tool;
use serde_json::{json, Value};
use std::sync::Mutex;

// Tests point HOME at a temp dir, which is process-wide.
static TEST_LOCK: Mutex<()> = Mutex::new(());

/// A HOME with one registered project, `alpha`, at `<home>/alpha` (no graph built).
fn with_registered_project(f: impl FnOnce(&std::path::Path)) {
    let _lock = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let home = tempfile::tempdir().unwrap();
    let project = home.path().join("alpha");
    std::fs::create_dir_all(&project).unwrap();
    let project = project.canonicalize().unwrap();
    let config = json!({
        "version": "1",
        "projects": {
            "alpha": {
                "slug": "alpha",
                "path": project.to_str().unwrap(),
                "last_build": null,
                "languages": [],
                "registered": "2024-01-01T00:00:00Z",
                "auto_watch": false,
                "exclude": []
            }
        },
        "global_exclude": [],
        "defaults": { "embedding_model": "minilm", "watch_debounce": 2000, "max_depth": 5, "exclude_tests": true },
        "mcp": { "transport": "stdio", "http_port": 37378, "restrict_to_projects": null }
    });
    let graphmind_dir = home.path().join(".graphmind");
    std::fs::create_dir_all(graphmind_dir.join("memory")).unwrap();
    std::fs::write(graphmind_dir.join("config.json"), config.to_string()).unwrap();
    unsafe { std::env::set_var("HOME", home.path()) };
    f(&project);
}

fn text(v: &Value) -> &str {
    v["content"][0]["text"].as_str().unwrap_or("")
}

fn is_error(v: &Value) -> bool {
    v.get("isError").and_then(Value::as_bool).unwrap_or(false)
}

#[test]
fn unknown_project_is_an_error_naming_it() {
    with_registered_project(|_| {
        for tool in ["gm_status", "gm_map", "gm_context", "gm_diff_impact"] {
            let resp = dispatch_tool(tool, &json!({ "project": "no-such-project" }));
            assert!(is_error(&resp), "{tool}: {resp}");
            assert_eq!(text(&resp), "Project 'no-such-project' is not registered (see `graphmind list`) Registered: alpha", "{tool}");
        }
    });
}

#[test]
fn unregistered_directory_is_an_error_naming_it() {
    with_registered_project(|project| {
        let outside = project.parent().unwrap().join("unregistered");
        std::fs::create_dir_all(&outside).unwrap();
        let outside = outside.to_str().unwrap();
        let resp = dispatch_tool("gm_context", &json!({ "project": outside }));
        assert!(is_error(&resp), "{resp}");
        assert_eq!(text(&resp), format!("Project '{outside}' is not registered (see `graphmind list`) Registered: alpha"));
    });
}

#[test]
fn registered_slug_and_checkout_path_resolve() {
    with_registered_project(|project| {
        let subdir = project.join("src");
        std::fs::create_dir_all(&subdir).unwrap();
        for given in ["alpha", project.to_str().unwrap(), subdir.to_str().unwrap()] {
            let resp = dispatch_tool("gm_context", &json!({ "project": given }));
            assert!(!is_error(&resp), "{given}: {}", text(&resp));
            assert!(text(&resp).contains("\"project\": \"alpha\""), "{given}: {}", text(&resp));
        }
    });
}

/// Arguments for a tool with `project` set and every other required parameter filled in, plus
/// `symbol` (gm_export needs a file or a symbol).
fn args_with_project(schema: &Value, project: &str) -> Value {
    let mut args = json!({ "project": project });
    let required = schema["required"].as_array().into_iter().flatten().filter_map(Value::as_str);
    let symbol = schema["properties"].get("symbol").map(|_| "symbol");
    for name in required.chain(symbol) {
        if name == "project" {
            continue;
        }
        args[name] = match schema["properties"][name]["type"].as_str() {
            Some("array") => json!([{ "content": "a fact" }]),
            Some("integer") => json!(1),
            _ => json!("wallet"),
        };
    }
    args
}

/// Every tool that takes `project` answers an unknown one with the same error, saves nothing,
/// and the server answers the next call.
#[test]
fn every_tool_taking_a_project_rejects_an_unknown_one() {
    with_registered_project(|project| {
        let memory_dir = project.parent().unwrap().join(".graphmind/memory");
        let expected = "Project 'no-such-project' is not registered (see `graphmind list`) Registered: alpha";
        let mut checked = Vec::new();
        for tool in graphmind_mcp::server::tool_defs() {
            let schema = Value::Object((*tool.input_schema).clone());
            if schema["properties"].get("project").is_none() {
                continue;
            }
            let args = args_with_project(&schema, "no-such-project");
            let resp = dispatch_tool(&tool.name, &args);
            assert!(is_error(&resp), "{} accepted {args}: {}", tool.name, text(&resp));
            assert_eq!(text(&resp), expected, "{}", tool.name);

            let next = dispatch_tool("gm_list_projects", &json!({}));
            assert!(!is_error(&next) && text(&next).contains("alpha"), "after {}: {next}", tool.name);
            checked.push(tool.name.to_string());
        }
        for tool in [
            "gm_search", "gm_listeners", "gm_cross_deps", "gm_cross_links", "gm_memory_search",
            "gm_memory_add", "gm_memory_update", "gm_memory_list", "gm_session_analyze", "gm_map",
        ] {
            assert!(checked.iter().any(|c| c == tool), "{tool} takes no project: {checked:?}");
        }
        let saved: Vec<_> = std::fs::read_dir(&memory_dir).unwrap().flatten().map(|e| e.file_name()).collect();
        assert!(saved.is_empty(), "memory saved for an unknown project: {saved:?}");
    });
}

/// Without `project`, global memory and the cross-project tools still answer.
#[test]
fn tools_without_a_project_still_answer() {
    with_registered_project(|_| {
        let resp = dispatch_tool("gm_memory_add", &json!({ "content": "a global fact", "global": true }));
        assert!(!is_error(&resp), "{}", text(&resp));
        let resp = dispatch_tool("gm_memory_search", &json!({ "query": "global fact" }));
        assert!(text(&resp).contains("a global fact"), "{}", text(&resp));
        for (tool, args) in [
            ("gm_cross_links", json!({})),
            ("gm_cross_deps", json!({ "project": "alpha" })),
            ("gm_cross_links", json!({ "project": "alpha" })),
            ("gm_search", json!({ "query": "anything", "project": "alpha" })),
            ("gm_listeners", json!({ "event": "anything", "project": "alpha" })),
        ] {
            let resp = dispatch_tool(tool, &args);
            assert!(!is_error(&resp), "{tool} {args}: {}", text(&resp));
        }
    });
}

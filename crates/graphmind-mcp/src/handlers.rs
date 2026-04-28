use graphmind_db::queries::GraphQueries;
use graphmind_db::schema::init_database;
use graphmind_memory::cross_links::CrossLinkStore;
use graphmind_memory::search::search as memory_search;
use graphmind_memory::store::{AddOptions, MemoryStore, MemoryType};
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;

// ---------------------------------------------------------------------------
// Config helpers
// ---------------------------------------------------------------------------

fn graphmind_dir() -> PathBuf {
    dirs::home_dir().unwrap_or_default().join(".graphmind")
}

fn config_path() -> PathBuf {
    graphmind_dir().join("config.json")
}

fn graphs_dir() -> PathBuf {
    graphmind_dir().join("graphs")
}

fn graph_db_path(slug: &str) -> PathBuf {
    graphs_dir().join(slug).join("graph.db")
}

fn memory_dir() -> PathBuf {
    graphmind_dir().join("memory")
}

fn cross_links_path() -> PathBuf {
    graphmind_dir()
        .join("cross-links")
        .join("links.jsonl")
}

fn meta_path(slug: &str) -> PathBuf {
    graphs_dir().join(slug).join("meta.json")
}

#[derive(Debug, Clone, Deserialize)]
struct ProjectConfig {
    path: String,
    slug: String,
    last_build: Option<String>,
    #[serde(default)]
    languages: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct GlobalConfig {
    #[serde(default)]
    projects: HashMap<String, ProjectConfig>,
}

fn load_config() -> GlobalConfig {
    let path = config_path();
    if !path.exists() {
        return GlobalConfig {
            projects: HashMap::new(),
        };
    }
    let content = std::fs::read_to_string(&path).unwrap_or_default();
    serde_json::from_str(&content).unwrap_or(GlobalConfig {
        projects: HashMap::new(),
    })
}

fn resolve_project(explicit: Option<&str>) -> Option<ProjectConfig> {
    let config = load_config();
    if let Some(slug) = explicit {
        return config.projects.get(slug).cloned();
    }
    config.projects.values().next().cloned()
}

fn all_project_slugs() -> Vec<String> {
    load_config().projects.keys().cloned().collect()
}

// ---------------------------------------------------------------------------
// Helpers for building text responses
// ---------------------------------------------------------------------------

const MAX_RESPONSE_BYTES: usize = 800_000;

fn text_content(text: &str) -> Value {
    json!({
        "content": [{ "type": "text", "text": text }]
    })
}

fn json_text(v: &Value) -> Value {
    let pretty = serde_json::to_string_pretty(v).unwrap_or_default();
    if pretty.len() <= MAX_RESPONSE_BYTES {
        return text_content(&pretty);
    }
    truncate_response(v)
}

fn truncate_response(v: &Value) -> Value {
    let obj = match v.as_object() {
        Some(o) => o,
        None => {
            let s = serde_json::to_string_pretty(v).unwrap_or_default();
            let truncated = &s[..s.len().min(MAX_RESPONSE_BYTES)];
            return text_content(&format!("{truncated}\n\n⚠ Response truncated. Use limit/offset parameters to paginate."));
        }
    };

    let mut shrunk = serde_json::Map::new();
    for (key, val) in obj {
        if let Some(arr) = val.as_array() {
            if serde_json::to_string(val).unwrap_or_default().len() > MAX_RESPONSE_BYTES / 2 {
                let take = (arr.len() / 4).max(10).min(arr.len());
                let truncated: Vec<Value> = arr.iter().take(take).cloned().collect();
                shrunk.insert(key.clone(), json!(truncated));
                shrunk.insert(
                    format!("{key}_pagination"),
                    json!({
                        "shown": take,
                        "total": arr.len(),
                        "next_offset": take,
                        "hint": format!("Use offset={} and limit={} to fetch the next page", take, take)
                    }),
                );
                continue;
            }
        }
        shrunk.insert(key.clone(), val.clone());
    }

    let result = Value::Object(shrunk);
    let pretty = serde_json::to_string_pretty(&result).unwrap_or_default();
    if pretty.len() > MAX_RESPONSE_BYTES {
        let truncated = &pretty[..MAX_RESPONSE_BYTES];
        text_content(&format!("{truncated}\n\n⚠ Response truncated. Use limit/offset parameters to paginate."))
    } else {
        text_content(&pretty)
    }
}

fn err_text(msg: &str) -> Value {
    json!({
        "content": [{ "type": "text", "text": msg }],
        "isError": true
    })
}

// ---------------------------------------------------------------------------
// Dispatcher
// ---------------------------------------------------------------------------

pub fn dispatch_tool(name: &str, args: &Value) -> Value {
    match name {
        // Graph
        "gm_query" => handle_query(args),
        "gm_fn" => handle_fn(args),
        "gm_deps" => handle_deps(args),
        "gm_impact" => handle_impact(args),
        "gm_fn_impact" => handle_fn_impact(args),
        "gm_map" => handle_map(args),
        "gm_cycles" => handle_cycles(args),
        // Memory
        "gm_memory_search" => handle_memory_search(args),
        "gm_memory_add" => handle_memory_add(args),
        "gm_memory_list" => handle_memory_list(args),
        // Meta
        "gm_list_projects" => handle_list_projects(args),
        "gm_status" => handle_status(args),
        "gm_context" => handle_context(args),
        // Cross
        "gm_cross_query" => handle_cross_query(args),
        "gm_cross_deps" => handle_cross_deps(args),
        "gm_cross_links" => handle_cross_links(args),
        "gm_diff_impact" => handle_diff_impact(args),
        // Search
        "gm_search" => handle_search(args),
        _ => err_text(&format!("Unknown tool: {name}")),
    }
}

// ---------------------------------------------------------------------------
// Graph tool handlers
// ---------------------------------------------------------------------------

fn with_graph<F>(args: &Value, f: F) -> Value
where
    F: FnOnce(&GraphQueries, &ProjectConfig) -> Value,
{
    let project_slug = args.get("project").and_then(|v| v.as_str());
    let proj = match resolve_project(project_slug) {
        Some(p) => p,
        None => return err_text("No project found. Register one with `graphmind init`."),
    };
    let db_path = graph_db_path(&proj.slug);
    if !db_path.exists() {
        return err_text(&format!(
            "No graph database for project '{}'. Run `graphmind build` first.",
            proj.slug
        ));
    }
    let db_path_str = db_path.to_string_lossy().to_string();
    let conn = match init_database(&db_path_str) {
        Ok(c) => c,
        Err(e) => return err_text(&format!("Failed to open graph db: {e}")),
    };
    let gq = GraphQueries::new(&conn);
    f(&gq, &proj)
}

fn handle_query(args: &Value) -> Value {
    let symbol = match args.get("symbol").and_then(|v| v.as_str()) {
        Some(s) => s,
        None => return err_text("Missing required parameter: symbol"),
    };
    let limit = args.get("limit").and_then(|v| v.as_u64()).unwrap_or(50) as usize;
    let offset = args.get("offset").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
    let project_slug = args.get("project").and_then(|v| v.as_str());

    if project_slug.is_some() {
        return with_graph(args, |gq, proj| {
            query_symbol_in_project(gq, &proj.slug, symbol, limit, offset)
        });
    }

    // No project specified: try resolved project first, then all projects
    if let Some(proj) = resolve_project(None) {
        let db_path = graph_db_path(&proj.slug);
        if db_path.exists() {
            if let Ok(conn) = init_database(&db_path.to_string_lossy()) {
                let gq = GraphQueries::new(&conn);
                let symbols = gq.find_symbol(symbol);
                if !symbols.is_empty() {
                    return query_symbol_in_project(&gq, &proj.slug, symbol, limit, offset);
                }
            }
        }
    }

    // Search all projects
    let slugs = all_project_slugs();
    let mut results: Vec<Value> = Vec::new();
    for slug in &slugs {
        let db_path = graph_db_path(slug);
        if !db_path.exists() { continue; }
        let conn = match init_database(&db_path.to_string_lossy()) {
            Ok(c) => c,
            Err(_) => continue,
        };
        let gq = GraphQueries::new(&conn);
        let found = gq.find_symbol(symbol);
        if !found.is_empty() {
            let callers = gq.callers(symbol);
            let callees = gq.callees(symbol);
            results.push(json!({
                "project": slug,
                "definitions": found,
                "callers": callers.iter().skip(offset).take(limit).collect::<Vec<_>>(),
                "callees": callees.iter().skip(offset).take(limit).collect::<Vec<_>>(),
                "total_callers": callers.len(),
                "total_callees": callees.len(),
            }));
        }
    }
    if results.is_empty() {
        return json_text(&json!({ "symbol": symbol, "message": "Symbol not found in any project." }));
    }
    json_text(&json!({
        "symbol": symbol,
        "results": results,
        "projects_searched": slugs.len()
    }))
}

fn query_symbol_in_project(gq: &GraphQueries, slug: &str, symbol: &str, limit: usize, offset: usize) -> Value {
    let symbols = gq.find_symbol(symbol);
    let all_callers = gq.callers(symbol);
    let all_callees = gq.callees(symbol);
    let callers: Vec<_> = all_callers.iter().skip(offset).take(limit).collect();
    let callees: Vec<_> = all_callees.iter().skip(offset).take(limit).collect();
    json_text(&json!({
        "project": slug,
        "symbol": symbol,
        "definitions": symbols,
        "callers": callers,
        "callees": callees,
        "total_callers": all_callers.len(),
        "total_callees": all_callees.len(),
        "limit": limit,
        "offset": offset
    }))
}

fn handle_fn(args: &Value) -> Value {
    let symbol = match args.get("symbol").and_then(|v| v.as_str()) {
        Some(s) => s,
        None => return err_text("Missing required parameter: symbol"),
    };
    let limit = args.get("limit").and_then(|v| v.as_u64()).unwrap_or(50) as usize;
    let offset = args.get("offset").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
    let project_slug = args.get("project").and_then(|v| v.as_str());

    if project_slug.is_some() {
        return with_graph(args, |gq, proj| {
            query_symbol_in_project(gq, &proj.slug, symbol, limit, offset)
        });
    }

    if let Some(proj) = resolve_project(None) {
        let db_path = graph_db_path(&proj.slug);
        if db_path.exists() {
            if let Ok(conn) = init_database(&db_path.to_string_lossy()) {
                let gq = GraphQueries::new(&conn);
                let symbols = gq.find_symbol(symbol);
                if !symbols.is_empty() {
                    return query_symbol_in_project(&gq, &proj.slug, symbol, limit, offset);
                }
            }
        }
    }

    let slugs = all_project_slugs();
    let mut results: Vec<Value> = Vec::new();
    for slug in &slugs {
        let db_path = graph_db_path(slug);
        if !db_path.exists() { continue; }
        let conn = match init_database(&db_path.to_string_lossy()) {
            Ok(c) => c,
            Err(_) => continue,
        };
        let gq = GraphQueries::new(&conn);
        let found = gq.find_symbol(symbol);
        if !found.is_empty() {
            let callers = gq.callers(symbol);
            let callees = gq.callees(symbol);
            results.push(json!({
                "project": slug,
                "definitions": found,
                "callers": callers.iter().skip(offset).take(limit).collect::<Vec<_>>(),
                "callees": callees.iter().skip(offset).take(limit).collect::<Vec<_>>(),
                "total_callers": callers.len(),
                "total_callees": callees.len(),
            }));
        }
    }
    if results.is_empty() {
        return json_text(&json!({ "symbol": symbol, "message": "Symbol not found in any project." }));
    }
    json_text(&json!({
        "symbol": symbol,
        "results": results,
        "projects_searched": slugs.len()
    }))
}

fn handle_deps(args: &Value) -> Value {
    let file = match args.get("file").and_then(|v| v.as_str()) {
        Some(s) => s,
        None => return err_text("Missing required parameter: file"),
    };
    let limit = args.get("limit").and_then(|v| v.as_u64()).unwrap_or(100) as usize;
    with_graph(args, |gq, _proj| {
        let deps = gq.file_deps(file);
        let reverse = gq.file_reverse_deps(file);
        let all_symbols = gq.symbols_in_file(file);
        let symbols: Vec<_> = all_symbols.iter().take(limit).collect();
        json_text(&json!({
            "file": file,
            "dependencies": deps,
            "dependents": reverse,
            "symbols": symbols,
            "total_symbols": all_symbols.len(),
            "limit": limit
        }))
    })
}

fn handle_impact(args: &Value) -> Value {
    let file = match args.get("file").and_then(|v| v.as_str()) {
        Some(s) => s,
        None => return err_text("Missing required parameter: file"),
    };
    let depth = args
        .get("depth")
        .and_then(|v| v.as_u64())
        .unwrap_or(5) as usize;
    with_graph(args, |gq, _proj| {
        let impacted = gq.impact(file, depth);
        json_text(&json!({
            "file": file,
            "depth": depth,
            "impacted_files": impacted,
            "count": impacted.len()
        }))
    })
}

fn handle_fn_impact(args: &Value) -> Value {
    let symbol = match args.get("symbol").and_then(|v| v.as_str()) {
        Some(s) => s,
        None => return err_text("Missing required parameter: symbol"),
    };
    with_graph(args, |gq, _proj| {
        let callers = gq.callers(symbol);
        let files: Vec<&str> = callers.iter().map(|c| c.file.as_str()).collect::<std::collections::HashSet<_>>().into_iter().collect();
        json_text(&json!({
            "symbol": symbol,
            "callers": callers,
            "affected_files": files,
            "caller_count": callers.len(),
            "file_count": files.len()
        }))
    })
}

fn handle_map(args: &Value) -> Value {
    let limit = args
        .get("limit")
        .and_then(|v| v.as_i64())
        .unwrap_or(20);
    with_graph(args, |gq, _proj| {
        let top = gq.top_connected(limit);
        json_text(&json!({
            "top_connected_files": top,
            "count": top.len()
        }))
    })
}

fn handle_cycles(args: &Value) -> Value {
    with_graph(args, |gq, _proj| {
        let cycles = gq.detect_cycles();
        json_text(&json!({
            "cycles": cycles,
            "count": cycles.len()
        }))
    })
}

// ---------------------------------------------------------------------------
// Memory tool handlers
// ---------------------------------------------------------------------------

fn handle_memory_search(args: &Value) -> Value {
    let query = match args.get("query").and_then(|v| v.as_str()) {
        Some(q) => q,
        None => return err_text("Missing required parameter: query"),
    };
    let limit = args
        .get("limit")
        .and_then(|v| v.as_u64())
        .unwrap_or(20) as usize;
    let project_slug = args.get("project").and_then(|v| v.as_str());

    let store = MemoryStore::new(&memory_dir());
    let entries = store.list(project_slug);
    let results = memory_search(&entries, query, limit);
    json_text(&json!({
        "query": query,
        "results": results,
        "count": results.len()
    }))
}

fn handle_memory_add(args: &Value) -> Value {
    let content = match args.get("content").and_then(|v| v.as_str()) {
        Some(c) => c,
        None => return err_text("Missing required parameter: content"),
    };
    let confirmed = args.get("confirmed").and_then(|v| v.as_bool()).unwrap_or(false);
    let project = args.get("project").and_then(|v| v.as_str()).map(String::from);
    let global = args.get("global").and_then(|v| v.as_bool()).unwrap_or(false);
    let tags: Vec<String> = args
        .get("tags")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default();

    let entry_type_str = args.get("type").and_then(|v| v.as_str()).unwrap_or("context");
    let entry_type = match entry_type_str {
        "decision" => MemoryType::Decision,
        "pattern" => MemoryType::Pattern,
        "convention" => MemoryType::Convention,
        "bug" => MemoryType::Bug,
        "session" => MemoryType::Session,
        _ => MemoryType::Context,
    };

    if !confirmed {
        return json_text(&json!({
            "confirmation_required": true,
            "preview": {
                "content": content,
                "project": project,
                "global": global,
                "type": entry_type_str,
                "tags": tags,
            },
            "message": "Call gm_memory_add again with confirmed: true to save."
        }));
    }

    let store = MemoryStore::new(&memory_dir());
    let entry = store.add(
        content,
        AddOptions {
            project,
            global,
            entry_type,
            tags,
        },
    );
    json_text(&json!({
        "saved": true,
        "id": entry.id,
        "created": entry.created
    }))
}

fn handle_memory_list(args: &Value) -> Value {
    let project_slug = args.get("project").and_then(|v| v.as_str());
    let limit = args
        .get("limit")
        .and_then(|v| v.as_u64())
        .unwrap_or(20) as usize;

    let store = MemoryStore::new(&memory_dir());
    let entries = store.list(project_slug);
    let truncated: Vec<_> = entries.into_iter().take(limit).collect();
    json_text(&json!({
        "entries": truncated,
        "count": truncated.len()
    }))
}

// ---------------------------------------------------------------------------
// Meta tool handlers
// ---------------------------------------------------------------------------

fn handle_list_projects(_args: &Value) -> Value {
    let config = load_config();
    let projects: Vec<Value> = config
        .projects
        .values()
        .map(|p| {
            json!({
                "slug": p.slug,
                "path": p.path,
                "last_build": p.last_build,
                "languages": p.languages
            })
        })
        .collect();
    json_text(&json!({
        "projects": projects,
        "count": projects.len()
    }))
}

fn handle_status(args: &Value) -> Value {
    with_graph(args, |gq, proj| {
        let stats = gq.stats();
        let langs = gq.language_breakdown();

        let meta: Value = if meta_path(&proj.slug).exists() {
            std::fs::read_to_string(meta_path(&proj.slug))
                .ok()
                .and_then(|c| serde_json::from_str(&c).ok())
                .unwrap_or(Value::Null)
        } else {
            Value::Null
        };

        json_text(&json!({
            "project": proj.slug,
            "path": proj.path,
            "stats": stats,
            "languages": langs,
            "last_build": proj.last_build,
            "meta": meta
        }))
    })
}

fn handle_context(args: &Value) -> Value {
    let project_slug = args.get("project").and_then(|v| v.as_str());
    let proj = match resolve_project(project_slug) {
        Some(p) => p,
        None => return err_text("No project found."),
    };

    // Graph stats
    let graph_info = {
        let db_path = graph_db_path(&proj.slug);
        if db_path.exists() {
            let db_path_str = db_path.to_string_lossy().to_string();
            if let Ok(conn) = init_database(&db_path_str) {
                let gq = GraphQueries::new(&conn);
                let stats = gq.stats();
                let langs = gq.language_breakdown();
                json!({ "stats": stats, "languages": langs })
            } else {
                json!({ "error": "Failed to open graph db" })
            }
        } else {
            json!({ "error": "No graph database" })
        }
    };

    // Recent memory
    let store = MemoryStore::new(&memory_dir());
    let recent_memory: Vec<_> = store.list(Some(&proj.slug)).into_iter().take(10).collect();

    // Cross links
    let cl_store = CrossLinkStore::new(&cross_links_path());
    let cross_links = cl_store.find_by_project(&proj.slug);

    json_text(&json!({
        "project": proj.slug,
        "path": proj.path,
        "graph": graph_info,
        "recent_memory": recent_memory,
        "cross_links": cross_links
    }))
}

// ---------------------------------------------------------------------------
// Cross-project tool handlers
// ---------------------------------------------------------------------------

fn handle_cross_query(args: &Value) -> Value {
    let symbol = match args.get("symbol").and_then(|v| v.as_str()) {
        Some(s) => s,
        None => return err_text("Missing required parameter: symbol"),
    };

    let slugs = all_project_slugs();
    let mut results: Vec<Value> = Vec::new();

    for slug in &slugs {
        let db_path = graph_db_path(slug);
        if !db_path.exists() {
            continue;
        }
        let db_path_str = db_path.to_string_lossy().to_string();
        let conn = match init_database(&db_path_str) {
            Ok(c) => c,
            Err(_) => continue,
        };
        let gq = GraphQueries::new(&conn);
        let found = gq.find_symbol(symbol);
        if !found.is_empty() {
            results.push(json!({
                "project": slug,
                "symbols": found
            }));
        }
    }

    json_text(&json!({
        "symbol": symbol,
        "results": results,
        "projects_searched": slugs.len()
    }))
}

fn handle_cross_deps(args: &Value) -> Value {
    let project = match args.get("project").and_then(|v| v.as_str()) {
        Some(p) => p,
        None => return err_text("Missing required parameter: project"),
    };

    let cl_store = CrossLinkStore::new(&cross_links_path());
    let links = cl_store.find_by_project(project);

    json_text(&json!({
        "project": project,
        "cross_dependencies": links,
        "count": links.len()
    }))
}

fn handle_cross_links(_args: &Value) -> Value {
    let cl_store = CrossLinkStore::new(&cross_links_path());
    let links = cl_store.list();

    json_text(&json!({
        "links": links,
        "count": links.len()
    }))
}

fn handle_diff_impact(args: &Value) -> Value {
    let depth = args.get("depth").and_then(|v| v.as_u64()).unwrap_or(5) as usize;
    let project_slug = args.get("project").and_then(|v| v.as_str());
    let proj = match resolve_project(project_slug) {
        Some(p) => p,
        None => return err_text("No project found."),
    };

    // Get changed files from git
    let project_path = Path::new(&proj.path);
    let output = match Command::new("git")
        .args(["diff", "--name-only", "HEAD"])
        .current_dir(project_path)
        .output()
    {
        Ok(o) => o,
        Err(e) => return err_text(&format!("Failed to run git diff: {e}")),
    };

    let stdout = String::from_utf8_lossy(&output.stdout);
    let changed_files: Vec<&str> = stdout.lines().filter(|l| !l.is_empty()).collect();

    if changed_files.is_empty() {
        return json_text(&json!({
            "project": proj.slug,
            "changed_files": [],
            "impacted_files": [],
            "message": "No uncommitted changes detected"
        }));
    }

    // For each changed file, find its impact
    let db_path = graph_db_path(&proj.slug);
    if !db_path.exists() {
        return err_text("No graph database. Run `graphmind build` first.");
    }
    let db_path_str = db_path.to_string_lossy().to_string();
    let conn = match init_database(&db_path_str) {
        Ok(c) => c,
        Err(e) => return err_text(&format!("Failed to open graph db: {e}")),
    };
    let gq = GraphQueries::new(&conn);

    let mut all_impacted: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut file_impacts: Vec<Value> = Vec::new();

    for file in &changed_files {
        let impacted = gq.impact(file, depth);
        for f in &impacted {
            all_impacted.insert(f.clone());
        }
        if !impacted.is_empty() {
            file_impacts.push(json!({
                "file": file,
                "impacted": impacted
            }));
        }
    }

    json_text(&json!({
        "project": proj.slug,
        "changed_files": changed_files,
        "file_impacts": file_impacts,
        "total_impacted": all_impacted.len(),
        "all_impacted_files": all_impacted.into_iter().collect::<Vec<_>>()
    }))
}

// ---------------------------------------------------------------------------
// Search tool handler
// ---------------------------------------------------------------------------

fn handle_search(args: &Value) -> Value {
    let raw_query = match args.get("query").and_then(|v| v.as_str()) {
        Some(q) => q,
        None => return err_text("Missing required parameter: query"),
    };
    let fts_query: String = raw_query
        .split(';')
        .flat_map(|part| part.split_whitespace())
        .filter(|w| w.len() > 1)
        .map(|w| format!("{w}*"))
        .collect::<Vec<_>>()
        .join(" OR ");
    if fts_query.is_empty() {
        return err_text("Empty search query after filtering");
    }
    let query = &fts_query;
    let limit = args
        .get("limit")
        .and_then(|v| v.as_i64())
        .unwrap_or(20);
    let kind_filter = args.get("kind").and_then(|v| v.as_str());
    let include_content = args.get("include_content").and_then(|v| v.as_bool()).unwrap_or(false);
    let project_slug = args.get("project").and_then(|v| v.as_str());

    let slugs = if let Some(slug) = project_slug {
        vec![slug.to_string()]
    } else {
        all_project_slugs()
    };

    let mut all_results: Vec<Value> = Vec::new();
    let mut total_found = 0;

    for slug in &slugs {
        let db_path = graph_db_path(slug);
        if !db_path.exists() {
            continue;
        }
        let db_path_str = db_path.to_string_lossy().to_string();
        let conn = match init_database(&db_path_str) {
            Ok(c) => c,
            Err(_) => continue,
        };
        let gq = GraphQueries::new(&conn);
        let raw_results = gq.search_symbols(query, limit * 5);
        let results: Vec<_> = if let Some(k) = kind_filter {
            raw_results.into_iter().filter(|r| r.kind.eq_ignore_ascii_case(k)).take(limit as usize).collect()
        } else {
            raw_results.into_iter().take(limit as usize).collect()
        };
        if !results.is_empty() {
            total_found += results.len();
            let compact: Vec<Value> = results.iter().map(|s| {
                let snippet = s.content.as_deref().map(|c| {
                    c.lines().take(5).collect::<Vec<_>>().join("\n")
                });
                let mut entry = json!({
                    "name": s.name,
                    "kind": s.kind,
                    "file": s.file,
                    "line_start": s.line_start,
                    "line_end": s.line_end,
                    "signature": s.signature,
                    "snippet": snippet,
                });
                if include_content {
                    entry.as_object_mut().unwrap().insert("content".to_string(), json!(s.content));
                }
                entry
            }).collect();
            all_results.push(json!({
                "project": slug,
                "symbols": compact
            }));
        }
    }

    json_text(&json!({
        "query": raw_query,
        "results": all_results,
        "projects_searched": slugs.len(),
        "total_found": total_found,
        "limit": limit
    }))
}

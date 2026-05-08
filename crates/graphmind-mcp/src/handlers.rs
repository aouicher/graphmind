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
// Update notice (cache-only, no network)
// ---------------------------------------------------------------------------

fn update_notice() -> Option<String> {
    #[derive(serde::Deserialize)]
    struct UpdateCache { latest_version: String, fetched_at: u64 }

    const CACHE_TTL_SECS: u64 = 86400;
    const CURRENT: &str = env!("CARGO_PKG_VERSION");

    let path = graphmind_config::paths::graphmind_dir().join("update-check.json");
    let raw = std::fs::read_to_string(&path).ok()?;
    let cache: UpdateCache = serde_json::from_str(&raw).ok()?;

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    if now - cache.fetched_at > CACHE_TTL_SECS || cache.latest_version.is_empty() {
        return None;
    }

    // Simple semver comparison: split on '.' and compare numerically
    fn parse(v: &str) -> (u64, u64, u64) {
        let mut p = v.trim_start_matches('v').splitn(3, '.');
        let a = p.next().and_then(|s| s.parse().ok()).unwrap_or(0);
        let b = p.next().and_then(|s| s.parse().ok()).unwrap_or(0);
        let c = p.next().and_then(|s| s.parse().ok()).unwrap_or(0);
        (a, b, c)
    }

    if parse(&cache.latest_version) > parse(CURRENT) {
        Some(format!(
            "⚠ graphmind update available: {} → {} — run `graphmind update`",
            CURRENT, cache.latest_version
        ))
    } else {
        None
    }
}

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

#[derive(Debug, Clone, Deserialize)]
struct ProjectConfig {
    path: String,
    slug: String,
    last_build: Option<String>,
    #[serde(default)]
    #[allow(dead_code)]
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

fn compact_symbol_line(s: &Value) -> String {
    let name = s.get("name").and_then(|v| v.as_str()).unwrap_or("?");
    let kind = s.get("kind").and_then(|v| v.as_str()).unwrap_or("?");
    let file = s.get("file").and_then(|v| v.as_str()).unwrap_or("?");
    let line = s.get("line_start").and_then(|v| v.as_i64()).unwrap_or(0);
    let sig = s.get("signature").and_then(|v| v.as_str()).unwrap_or("");
    if sig.is_empty() {
        format!("  {name} [{kind}] {file}:{line}")
    } else {
        format!("  {name} [{kind}] {file}:{line}\n    ({sig})")
    }
}

fn compact_edge_line(e: &Value) -> String {
    let name = e.get("name").and_then(|v| v.as_str()).unwrap_or("?");
    let kind = e.get("kind").and_then(|v| v.as_str()).unwrap_or("?");
    let file = e.get("file").and_then(|v| v.as_str()).unwrap_or("?");
    let line = e.get("line_start").and_then(|v| v.as_i64()).unwrap_or(0);
    let sig = e.get("signature").and_then(|v| v.as_str()).unwrap_or("");
    if sig.is_empty() {
        format!("  {name} [{kind}] {file}:{line}")
    } else {
        format!("  {name} [{kind}] {file}:{line}\n    ({sig})")
    }
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
        "gm_listeners" => handle_listeners(args),
        // New tools
        "gm_outline" => handle_outline(args),
        "gm_who_calls_chain" => handle_who_calls_chain(args),
        "gm_dead_code" => handle_dead_code(args),
        "gm_export" => handle_export(args),
        "gm_similar" => handle_similar(args),
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
    handle_symbol_query(args, false)
}

fn handle_symbol_query(args: &Value, default_include_content: bool) -> Value {
    let symbol = match args.get("symbol").and_then(|v| v.as_str()) {
        Some(s) => s,
        None => return err_text("Missing required parameter: symbol"),
    };
    let limit = args.get("limit").and_then(|v| v.as_u64()).unwrap_or(15) as usize;
    let offset = args.get("offset").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
    let file_filter = args.get("file").and_then(|v| v.as_str());
    let kind_filter = args.get("kind").and_then(|v| v.as_str());
    let project_slug = args.get("project").and_then(|v| v.as_str());
    let include_content = args.get("include_content").and_then(|v| v.as_bool()).unwrap_or(default_include_content);
    let format = args.get("format").and_then(|v| v.as_str()).unwrap_or("compact");

    let opts = SymbolQueryOptions { file: file_filter, kind: kind_filter, limit, offset, include_content, format };

    if project_slug.is_some() {
        return with_graph(args, |gq, proj| {
            query_symbol_filtered(gq, &proj.slug, symbol, &opts)
        });
    }

    if let Some(proj) = resolve_project(None) {
        let db_path = graph_db_path(&proj.slug);
        if db_path.exists() {
            if let Ok(conn) = init_database(&db_path.to_string_lossy()) {
                let gq = GraphQueries::new(&conn);
                let symbols = gq.find_symbol_filtered(symbol, file_filter, kind_filter);
                if !symbols.is_empty() {
                    return query_symbol_filtered(&gq, &proj.slug, symbol, &opts);
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
        let found = gq.find_symbol_filtered(symbol, file_filter, kind_filter);
        if !found.is_empty() {
            let defs = qualify_definitions(&gq, &found, include_content);
            let callers = gq.callers_filtered(symbol, file_filter);
            let callees = gq.callees_filtered(symbol, file_filter);
            let compact_callers = compact_edges(&gq, &callers);
            let compact_callees = compact_edges(&gq, &callees);
            let callers_truncated = compact_callers.len() > limit;
            let callees_truncated = compact_callees.len() > limit;
            let mut entry = json!({
                "project": slug,
                "definitions": defs,
                "callers": compact_callers.iter().skip(offset).take(limit).collect::<Vec<_>>(),
                "callees": compact_callees.iter().skip(offset).take(limit).collect::<Vec<_>>(),
            });
            let obj = entry.as_object_mut().unwrap();
            if callers_truncated {
                obj.insert("callers_truncated".into(), json!(true));
                obj.insert("total_callers".into(), json!(compact_callers.len()));
            }
            if callees_truncated {
                obj.insert("callees_truncated".into(), json!(true));
                obj.insert("total_callees".into(), json!(compact_callees.len()));
            }
            results.push(entry);
        }
    }
    if results.is_empty() {
        return json_text(&json!({ "symbol": symbol, "message": "Symbol not found in any project." }));
    }
    json_text(&json!({
        "symbol": symbol,
        "results": results,
    }))
}

fn compact_edges(gq: &GraphQueries, edges: &[graphmind_db::queries::SymbolWithEdge]) -> Vec<Value> {
    let mut seen = std::collections::HashSet::new();
    edges.iter().filter(|e| seen.insert((e.name.clone(), e.file.clone(), e.line_start))).map(|e| {
        let snippet = e.content.as_deref().map(|c| {
            c.lines().take(3).collect::<Vec<_>>().join("\n")
        });
        let as_row = graphmind_db::queries::SymbolRow {
            id: e.id, name: e.name.clone(), kind: e.kind.clone(),
            file: e.file.clone(), line_start: e.line_start, line_end: e.line_end,
            signature: e.signature.clone(), doc: e.doc.clone(), content: e.content.clone(),
        };
        let qualified = gq.resolve_qualified_name(&as_row);
        let mut entry = json!({
            "name": e.name,
            "qualified_name": qualified,
            "kind": e.kind,
            "file": e.file,
            "line_start": e.line_start,
            "edge_kind": e.edge_kind,
        });
        let obj = entry.as_object_mut().unwrap();
        if let Some(ref sig) = e.signature {
            if !sig.is_empty() { obj.insert("signature".into(), json!(sig)); }
        }
        if let Some(ref s) = snippet {
            if !s.is_empty() { obj.insert("snippet".into(), json!(s)); }
        }
        entry
    }).collect()
}

fn qualify_definitions(gq: &GraphQueries, symbols: &[graphmind_db::queries::SymbolRow], include_content: bool) -> Vec<Value> {
    symbols.iter().map(|s| {
        let qualified = gq.resolve_qualified_name(s);
        let mut entry = json!({
            "name": s.name,
            "qualified_name": qualified,
            "kind": s.kind,
            "file": s.file,
            "line_start": s.line_start,
            "line_end": s.line_end,
        });
        let obj = entry.as_object_mut().unwrap();
        if let Some(ref sig) = s.signature {
            if !sig.is_empty() { obj.insert("signature".into(), json!(sig)); }
        }
        if let Some(ref doc) = s.doc {
            if !doc.is_empty() { obj.insert("doc".into(), json!(doc)); }
        }
        if include_content {
            if let Some(ref content) = s.content {
                obj.insert("content".into(), json!(content));
            }
        }
        entry
    }).collect()
}

struct SymbolQueryOptions<'a> {
    file: Option<&'a str>,
    kind: Option<&'a str>,
    limit: usize,
    offset: usize,
    include_content: bool,
    format: &'a str,
}

impl<'a> Default for SymbolQueryOptions<'a> {
    fn default() -> Self {
        Self {
            file: None,
            kind: None,
            limit: 15,
            offset: 0,
            include_content: false,
            format: "compact",
        }
    }
}

fn query_symbol_filtered(gq: &GraphQueries, slug: &str, symbol: &str, opts: &SymbolQueryOptions<'_>) -> Value {
    let file = opts.file;
    let kind = opts.kind;
    let limit = opts.limit;
    let offset = opts.offset;
    let include_content = opts.include_content;
    let format = opts.format;
    let symbols = gq.find_symbol_filtered(symbol, file, kind);
    let definitions = qualify_definitions(gq, &symbols, include_content);
    let all_callers = gq.callers_filtered(symbol, file);
    let all_callees = gq.callees_filtered(symbol, file);
    let compact_callers = compact_edges(gq, &all_callers);
    let compact_callees = compact_edges(gq, &all_callees);
    let callers_truncated = compact_callers.len() > limit;
    let callees_truncated = compact_callees.len() > limit;
    let callers: Vec<_> = compact_callers.iter().skip(offset).take(limit).collect();
    let callees: Vec<_> = compact_callees.iter().skip(offset).take(limit).collect();

    if format == "compact" {
        let mut lines = Vec::new();
        lines.push(format!("# {symbol} [{slug}]"));
        for d in &definitions {
            lines.push(compact_symbol_line(d));
            if include_content {
                if let Some(content) = d.get("content").and_then(|v| v.as_str()) {
                    if !content.is_empty() {
                        let content_lines: Vec<&str> = content.lines().collect();
                        if content_lines.len() <= 100 {
                            lines.push(format!("\n```\n{content}\n```"));
                        } else {
                            let head: String = content_lines[..60].join("\n");
                            let tail: String = content_lines[content_lines.len()-20..].join("\n");
                            lines.push(format!("\n```\n{head}\n// ... ({} lines total)\n{tail}\n```", content_lines.len()));
                        }
                    }
                }
            }
        }
        if !callers.is_empty() {
            let trunc = if callers_truncated { format!(" (showing {}/{}", limit, compact_callers.len()) + ")" } else { String::new() };
            lines.push(format!("\nCallers{}:", trunc));
            for c in &callers { lines.push(compact_edge_line(c)); }
        }
        if !callees.is_empty() {
            let trunc = if callees_truncated { format!(" (showing {}/{}", limit, compact_callees.len()) + ")" } else { String::new() };
            lines.push(format!("\nCallees{}:", trunc));
            for c in &callees { lines.push(compact_edge_line(c)); }
        }
        return text_content(&lines.join("\n"));
    }

    let mut result = json!({
        "project": slug,
        "symbol": symbol,
        "definitions": definitions,
        "callers": callers,
        "callees": callees,
    });
    let obj = result.as_object_mut().unwrap();
    if callers_truncated {
        obj.insert("callers_truncated".into(), json!(true));
        obj.insert("total_callers".into(), json!(compact_callers.len()));
    }
    if callees_truncated {
        obj.insert("callees_truncated".into(), json!(true));
        obj.insert("total_callees".into(), json!(compact_callees.len()));
    }
    json_text(&result)
}

fn handle_fn(args: &Value) -> Value {
    handle_symbol_query(args, true)
}

fn handle_deps(args: &Value) -> Value {
    let file = match args.get("file").and_then(|v| v.as_str()) {
        Some(s) => s,
        None => return err_text("Missing required parameter: file"),
    };
    let limit = args.get("limit").and_then(|v| v.as_u64()).unwrap_or(30) as usize;
    with_graph(args, |gq, _proj| {
        let deps = gq.file_deps(file);
        let reverse = gq.file_reverse_deps(file);
        let all_symbols = gq.symbols_in_file(file);

        let mut lines = vec![format!(">> deps: {}\n", file)];

        if !deps.is_empty() {
            lines.push(format!("  Imports ({}):", deps.len()));
            for d in deps.iter().take(limit) {
                lines.push(format!("    → {} [{}] ({}x)", d.file, d.kind, d.count));
            }
            if deps.len() > limit {
                lines.push(format!("    ... +{} more", deps.len() - limit));
            }
        }

        if !reverse.is_empty() {
            lines.push(format!("\n  Imported by ({}):", reverse.len()));
            for d in reverse.iter().take(limit) {
                lines.push(format!("    ← {} [{}] ({}x)", d.file, d.kind, d.count));
            }
            if reverse.len() > limit {
                lines.push(format!("    ... +{} more", reverse.len() - limit));
            }
        }

        if !all_symbols.is_empty() {
            lines.push(format!("\n  Symbols ({}):", all_symbols.len()));
            for s in all_symbols.iter().take(limit) {
                let sig = s.signature.as_deref().unwrap_or("");
                let sig_part = if sig.is_empty() { String::new() } else { format!("({})", sig) };
                lines.push(format!("    {} {}{} :{}", s.kind, s.name, sig_part, s.line_start));
            }
            if all_symbols.len() > limit {
                lines.push(format!("    ... +{} more", all_symbols.len() - limit));
            }
        }

        text_content(&lines.join("\n"))
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
    with_graph(args, |gq, proj| {
        let top = gq.top_connected(limit);
        let mut lines = vec![format!(">> {} top connected files [{}]:\n", top.len(), proj.slug)];
        for entry in &top {
            lines.push(format!("  {} ({} connections)", entry.file, entry.connections));
        }
        text_content(&lines.join("\n"))
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
    let mut lines = vec![format!(">> {} memory results for \"{}\":\n", results.len(), query)];
    for r in &results {
        let preview: String = r.content.lines().next().unwrap_or("").chars().take(100).collect();
        let r_type = format!("{:?}", r.entry_type).to_lowercase();
        lines.push(format!("  [{r_type}] {preview}"));
    }
    text_content(&lines.join("\n"))
}

fn handle_memory_add(args: &Value) -> Value {
    let content = match args.get("content").and_then(|v| v.as_str()) {
        Some(c) => c,
        None => return err_text("Missing required parameter: content"),
    };
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

    let priority = args.get("priority").and_then(|v| v.as_bool()).unwrap_or(false);

    let store = MemoryStore::new(&memory_dir());
    let entry = store.add(
        content,
        AddOptions {
            project,
            global,
            entry_type,
            tags,
            priority,
        },
    );
    let prio_str = if priority { " ★priority" } else { "" };
    text_content(&format!("✓ Saved to memory (id: {}, type: {}{})", entry.id, entry_type_str, prio_str))
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
    let mut lines = vec![format!(">> {} memory entries:\n", truncated.len())];
    for e in &truncated {
        let preview: String = e.content.lines().next().unwrap_or("").chars().take(80).collect();
        let e_type = format!("{:?}", e.entry_type).to_lowercase();
        lines.push(format!("  [{e_type}] {preview} (id: {})", e.id));
    }
    text_content(&lines.join("\n"))
}

// ---------------------------------------------------------------------------
// Meta tool handlers
// ---------------------------------------------------------------------------

fn handle_list_projects(_args: &Value) -> Value {
    let config = load_config();
    let mut lines = vec![format!(">> {} registered projects:\n", config.projects.len())];
    for p in config.projects.values() {
        let last = p.last_build.as_deref().unwrap_or("never");
        lines.push(format!("  {} — {} (built: {})", p.slug, p.path, last));
    }
    text_content(&lines.join("\n"))
}

fn handle_status(args: &Value) -> Value {
    with_graph(args, |gq, proj| {
        let stats = gq.stats();
        let langs = gq.language_breakdown();

        let symbols = stats.symbols;
        let edges = stats.edges;
        let files = stats.files;

        let lang_str: String = langs.iter()
            .map(|l| format!("{} ({})", l.language, l.count))
            .collect::<Vec<_>>()
            .join(", ");

        let last_build = proj.last_build.as_deref().unwrap_or("never");

        let mut text = format!(
            ">> Project: {}\n  Path: {}\n  Last build: {}\n  Graph: {} symbols, {} edges, {} files\n  Languages: {}",
            proj.slug, proj.path, last_build, symbols, edges, files, lang_str
        );
        if let Some(notice) = update_notice() {
            text.push('\n');
            text.push_str(&notice);
        }
        text_content(&text)
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

    let mut out = json!({
        "project": proj.slug,
        "path": proj.path,
        "graph": graph_info,
        "recent_memory": recent_memory,
        "cross_links": cross_links
    });
    if let Some(notice) = update_notice() {
        out["update_notice"] = json!(notice);
    }
    json_text(&out)
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
    let kind_raw = args.get("kind").and_then(|v| v.as_str());
    let kind_normalized: Option<String> = kind_raw.map(|k| {
        let mut c = k.chars();
        match c.next() {
            Some(first) => first.to_uppercase().to_string() + &c.as_str().to_lowercase(),
            None => String::new(),
        }
    });
    let kind_filter = kind_normalized.as_deref();
    let include_content = args.get("include_content").and_then(|v| v.as_bool()).unwrap_or(false);
    let format = args.get("format").and_then(|v| v.as_str()).unwrap_or("compact");
    let project_slug = args.get("project").and_then(|v| v.as_str());

    let slugs = if let Some(slug) = project_slug {
        vec![slug.to_string()]
    } else {
        all_project_slugs()
    };

    // Load embedding engine if configured
    let embed_engine = load_embed_engine();

    let mut all_results: Vec<Value> = Vec::new();

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
        let fts_results: Vec<_> = gq.search_symbols_filtered(query, limit * 3, kind_filter)
            .into_iter().take(limit as usize).collect();

        // Semantic search via embeddings (if available)
        let merged = if let Some(ref engine) = embed_engine {
            let emb_db_path = embedding_db_path(slug);
            if emb_db_path.exists() {
                let semantic_results = graphmind_embeddings::search::semantic_search(
                    &emb_db_path,
                    raw_query,
                    &|text| engine.embed(text).ok(),
                    limit as usize,
                    kind_filter,
                );
                fuse_fts_and_semantic(&fts_results, &semantic_results, limit as usize)
            } else {
                fts_results.into_iter().map(|s| FusedResult::from_symbol(s, None)).collect()
            }
        } else {
            fts_results.into_iter().map(|s| FusedResult::from_symbol(s, None)).collect()
        };

        if !merged.is_empty() {
            let compact: Vec<Value> = merged.iter().map(|r| {
                let mut entry = json!({
                    "name": r.name,
                    "kind": r.kind,
                    "file": r.file,
                    "line_start": r.line_start,
                    "score": r.score,
                });
                let obj = entry.as_object_mut().unwrap();
                if let Some(ref sig) = r.signature {
                    if !sig.is_empty() { obj.insert("signature".into(), json!(sig)); }
                }
                if let Some(ref snippet) = r.snippet {
                    if !snippet.is_empty() { obj.insert("snippet".into(), json!(snippet)); }
                }
                if include_content {
                    if let Some(ref content) = r.content {
                        obj.insert("content".into(), json!(content));
                    }
                }
                entry
            }).collect();
            all_results.push(json!({
                "project": slug,
                "symbols": compact
            }));
        }
    }

    let total_count: usize = all_results.iter().map(|r| r.get("symbols").and_then(|v| v.as_array()).map(|a| a.len()).unwrap_or(0)).sum();

    // Auto-expand: if exactly 1 result, add callers/callees (like gm_fn)
    if total_count == 1 {
        if let Some(first_project) = all_results.first() {
            let slug = first_project.get("project").and_then(|v| v.as_str()).unwrap_or("");
            if let Some(symbols) = first_project.get("symbols").and_then(|v| v.as_array()) {
                if let Some(sym) = symbols.first() {
                    let sym_name = sym.get("name").and_then(|v| v.as_str()).unwrap_or("");
                    let sym_file = sym.get("file").and_then(|v| v.as_str());
                    if !sym_name.is_empty() {
                        let db_path = graph_db_path(slug);
                        if let Ok(conn) = init_database(&db_path.to_string_lossy()) {
                            let gq = GraphQueries::new(&conn);
                            let callers = gq.callers_filtered(sym_name, sym_file);
                            let callees = gq.callees_filtered(sym_name, sym_file);
                            let compact_callers = compact_edges(&gq, &callers);
                            let compact_callees = compact_edges(&gq, &callees);

                            if format == "compact" {
                                let mut lines = vec![format!(">> 1 result for \"{}\" (auto-expanded):\n", raw_query)];
                                lines.push(compact_symbol_line(sym));
                                if !compact_callers.is_empty() {
                                    lines.push(format!("\n  Callers ({}):", compact_callers.len()));
                                    for c in compact_callers.iter().take(10) {
                                        lines.push(format!("    ← {}", compact_edge_line(c)));
                                    }
                                    if compact_callers.len() > 10 {
                                        lines.push(format!("    ... and {} more", compact_callers.len() - 10));
                                    }
                                }
                                if !compact_callees.is_empty() {
                                    lines.push(format!("\n  Callees ({}):", compact_callees.len()));
                                    for c in compact_callees.iter().take(10) {
                                        lines.push(format!("    → {}", compact_edge_line(c)));
                                    }
                                    if compact_callees.len() > 10 {
                                        lines.push(format!("    ... and {} more", compact_callees.len() - 10));
                                    }
                                }
                                return text_content(&lines.join("\n"));
                            } else {
                                let mut result = all_results.into_iter().next().unwrap();
                                let obj = result.as_object_mut().unwrap();
                                obj.insert("callers".into(), json!(compact_callers.iter().take(15).collect::<Vec<_>>()));
                                obj.insert("callees".into(), json!(compact_callees.iter().take(15).collect::<Vec<_>>()));
                                return json_text(&json!({
                                    "query": raw_query,
                                    "auto_expanded": true,
                                    "results": [result],
                                }));
                            }
                        }
                    }
                }
            }
        }
    }

    if format == "compact" {
        let mut lines = vec![format!(">> {} result(s) for \"{}\":\n", total_count, raw_query)];
        for r in &all_results {
            if let Some(symbols) = r.get("symbols").and_then(|v| v.as_array()) {
                for s in symbols {
                    lines.push(compact_symbol_line(s));
                }
            }
        }
        if total_count > 1 {
            lines.push(format!("\n({} matches — specify file= to narrow)", total_count));
        }
        text_content(&lines.join("\n"))
    } else {
        json_text(&json!({
            "query": raw_query,
            "results": all_results,
        }))
    }
}

// ---------------------------------------------------------------------------
// Embedding fusion helpers
// ---------------------------------------------------------------------------

fn embedding_db_path(slug: &str) -> std::path::PathBuf {
    graphs_dir().join(slug).join("embeddings.db")
}

fn load_embed_engine() -> Option<Box<dyn graphmind_embeddings::engine::EmbeddingEngine>> {
    let config = graphmind_config::config::load_config();
    if config.embedding.mode == graphmind_config::config::EmbeddingMode::Disabled {
        return None;
    }
    graphmind_embeddings::factory::create_engine(&config.embedding).ok()
}

struct FusedResult {
    name: String,
    kind: String,
    file: String,
    line_start: i64,
    signature: Option<String>,
    snippet: Option<String>,
    content: Option<String>,
    score: f64,
}

impl FusedResult {
    fn from_symbol(s: graphmind_db::queries::SymbolRow, score: Option<f64>) -> Self {
        let snippet = s.content.as_deref().map(|c| {
            c.lines().take(3).collect::<Vec<_>>().join("\n")
        });
        Self {
            name: s.name,
            kind: s.kind,
            file: s.file,
            line_start: s.line_start,
            signature: s.signature,
            snippet,
            content: s.content,
            score: score.unwrap_or(0.0),
        }
    }
}

fn fuse_fts_and_semantic(
    fts: &[graphmind_db::queries::SymbolRow],
    semantic: &[graphmind_embeddings::search::SearchResult],
    limit: usize,
) -> Vec<FusedResult> {
    use std::collections::HashMap;
    let k = 60.0;

    let mut scores: HashMap<String, FusedResult> = HashMap::new();

    // FTS ranking
    for (i, s) in fts.iter().enumerate() {
        let key = format!("{}:{}:{}", s.file, s.name, s.line_start);
        let rrf_score = 1.0 / (k + i as f64 + 1.0);
        let snippet = s.content.as_deref().map(|c| c.lines().take(3).collect::<Vec<_>>().join("\n"));
        scores.entry(key).or_insert_with(|| FusedResult {
            name: s.name.clone(),
            kind: s.kind.clone(),
            file: s.file.clone(),
            line_start: s.line_start,
            signature: s.signature.clone(),
            snippet,
            content: s.content.clone(),
            score: 0.0,
        }).score += rrf_score;
    }

    // Semantic ranking
    for (i, r) in semantic.iter().enumerate() {
        let key = format!("{}:{}:0", r.file, r.symbol_name);
        let rrf_score = 1.0 / (k + i as f64 + 1.0);
        scores.entry(key).or_insert_with(|| FusedResult {
            name: r.symbol_name.clone(),
            kind: r.symbol_kind.clone(),
            file: r.file.clone(),
            line_start: 0,
            signature: None,
            snippet: Some(r.text.lines().take(3).collect::<Vec<_>>().join("\n")),
            content: None,
            score: 0.0,
        }).score += rrf_score;
    }

    let mut results: Vec<FusedResult> = scores.into_values().collect();
    // Penalize test files so production code ranks higher
    for r in &mut results {
        if r.file.contains("/tests/") || r.file.contains("/test/") || r.file.contains("_test.") || r.file.contains(".test.") {
            r.score *= 0.5;
        }
    }
    results.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
    results.truncate(limit);
    results
}

// ---------------------------------------------------------------------------
// Listeners tool handler
// ---------------------------------------------------------------------------

fn handle_listeners(args: &Value) -> Value {
    let event = match args.get("event").and_then(|v| v.as_str()) {
        Some(e) => e,
        None => return err_text("Missing required parameter: event"),
    };
    let project_slug = args.get("project").and_then(|v| v.as_str());

    let slugs = if let Some(slug) = project_slug {
        vec![slug.to_string()]
    } else {
        all_project_slugs()
    };

    let mut all_results: Vec<Value> = Vec::new();

    for slug in &slugs {
        let db_path = graph_db_path(slug);
        if !db_path.exists() { continue; }
        let db_path_str = db_path.to_string_lossy().to_string();
        let conn = match init_database(&db_path_str) {
            Ok(c) => c,
            Err(_) => continue,
        };
        let gq = GraphQueries::new(&conn);
        let listeners = gq.find_listeners(event);
        if !listeners.is_empty() {
            let compact: Vec<Value> = listeners.iter().map(|s| {
                let qualified = gq.resolve_qualified_name(s);
                let snippet = s.content.as_deref().map(|c| {
                    c.lines().take(3).collect::<Vec<_>>().join("\n")
                });
                let mut entry = json!({
                    "name": s.name,
                    "qualified_name": qualified,
                    "kind": s.kind,
                    "file": s.file,
                    "line_start": s.line_start,
                });
                let obj = entry.as_object_mut().unwrap();
                if let Some(ref sig) = s.signature {
                    if !sig.is_empty() { obj.insert("signature".into(), json!(sig)); }
                }
                if let Some(ref snip) = snippet {
                    if !snip.is_empty() { obj.insert("snippet".into(), json!(snip)); }
                }
                entry
            }).collect();
            all_results.push(json!({
                "project": slug,
                "listeners": compact
            }));
        }
    }

    let mut lines = vec![format!(">> Listeners for \"{}\":\n", event)];
    for r in &all_results {
        let proj = r.get("project").and_then(|v| v.as_str()).unwrap_or("?");
        if let Some(listeners) = r.get("listeners").and_then(|v| v.as_array()) {
            for l in listeners {
                let name = l.get("name").and_then(|v| v.as_str()).unwrap_or("?");
                let kind = l.get("kind").and_then(|v| v.as_str()).unwrap_or("?");
                let file = l.get("file").and_then(|v| v.as_str()).unwrap_or("?");
                let line = l.get("line_start").and_then(|v| v.as_i64()).unwrap_or(0);
                let sig = l.get("signature").and_then(|v| v.as_str()).unwrap_or("");
                if sig.is_empty() {
                    lines.push(format!("  {name} [{kind}] {file}:{line} ({proj})"));
                } else {
                    lines.push(format!("  {name} [{kind}] {file}:{line} ({proj})\n    ({sig})"));
                }
            }
        }
    }
    if lines.len() == 1 {
        lines.push("  (none found)".to_string());
    }
    text_content(&lines.join("\n"))
}

// ---------------------------------------------------------------------------
// Outline tool handler
// ---------------------------------------------------------------------------

fn handle_outline(args: &Value) -> Value {
    let file = match args.get("file").and_then(|v| v.as_str()) {
        Some(f) => f,
        None => return err_text("Missing required parameter: file"),
    };
    with_graph(args, |gq, _proj| {
        let outline = gq.outline(file);
        let mut lines = vec![format!(">> outline: {}\n", file)];
        fn render_tree(nodes: &[graphmind_db::queries::OutlineNode], lines: &mut Vec<String>, depth: usize) {
            for node in nodes {
                let indent = "  ".repeat(depth);
                let sig = node.signature.as_deref().unwrap_or("");
                let sig_part = if sig.is_empty() { String::new() } else { format!("({})", sig) };
                lines.push(format!("{}{} {}{} :{}", indent, node.kind, node.name, sig_part, node.line_start));
                if !node.children.is_empty() {
                    render_tree(&node.children, lines, depth + 1);
                }
            }
        }
        render_tree(&outline, &mut lines, 1);
        text_content(&lines.join("\n"))
    })
}

// ---------------------------------------------------------------------------
// Who calls chain tool handler
// ---------------------------------------------------------------------------

fn handle_who_calls_chain(args: &Value) -> Value {
    let symbol = match args.get("symbol").and_then(|v| v.as_str()) {
        Some(s) => s,
        None => return err_text("Missing required parameter: symbol"),
    };
    let depth = args.get("depth").and_then(|v| v.as_u64()).unwrap_or(3) as usize;

    with_graph(args, |gq, _proj| {
        let (chain, max_reached) = gq.who_calls_chain(symbol, depth);
        let mut lines = vec![format!(">> who calls {} (depth {}, {} callers):\n", symbol, depth, chain.len())];
        for node in &chain {
            let indent = "  ".repeat(node.depth + 1);
            lines.push(format!("{}← {} [{}] {}:{}", indent, node.name, node.kind, node.file, node.line_start));
        }
        if max_reached {
            lines.push(format!("\n  (max depth {} reached — increase depth for more)", depth));
        }
        text_content(&lines.join("\n"))
    })
}

// ---------------------------------------------------------------------------
// Dead code tool handler
// ---------------------------------------------------------------------------

fn handle_dead_code(args: &Value) -> Value {
    let kind = args.get("kind").and_then(|v| v.as_str());
    let limit = args.get("limit").and_then(|v| v.as_i64()).unwrap_or(50);

    with_graph(args, |gq, proj| {
        let dead = gq.dead_code(kind, limit);
        let qualified = qualify_definitions(gq, &dead, false);
        let kind_label = kind.unwrap_or("all");
        let mut lines = vec![format!(">> {} dead symbols [{}] (kind: {}):\n", qualified.len(), proj.slug, kind_label)];
        for s in &qualified {
            lines.push(compact_symbol_line(s));
        }
        text_content(&lines.join("\n"))
    })
}

// ---------------------------------------------------------------------------
// Export tool handler (Mermaid / DOT)
// ---------------------------------------------------------------------------

fn handle_export(args: &Value) -> Value {
    let file = args.get("file").and_then(|v| v.as_str());
    let symbol = args.get("symbol").and_then(|v| v.as_str());
    let format = args.get("format").and_then(|v| v.as_str()).unwrap_or("mermaid");
    let depth = args.get("depth").and_then(|v| v.as_u64()).unwrap_or(2) as usize;

    if file.is_none() && symbol.is_none() {
        return err_text("At least one of 'file' or 'symbol' is required");
    }

    with_graph(args, |gq, proj| {
        let (symbols, edges) = if let Some(f) = file {
            gq.file_subgraph(f)
        } else {
            let sym_name = symbol.unwrap();
            let found = gq.find_symbol(sym_name);
            if found.is_empty() {
                return err_text(&format!("Symbol '{}' not found", sym_name));
            }
            gq.neighborhood(found[0].id, depth)
        };

        let output = match format {
            "dot" => render_dot(&symbols, &edges, &proj.slug),
            _ => render_mermaid(&symbols, &edges, &proj.slug),
        };

        text_content(&output)
    })
}

fn render_mermaid(symbols: &[graphmind_db::queries::SymbolRow], edges: &[graphmind_db::queries::EdgeRow], title: &str) -> String {
    let mut out = format!("flowchart LR\n  %% {title}\n");
    for s in symbols {
        let shape = match s.kind.as_str() {
            "Class" | "Interface" => format!("[{}]", s.name),
            "Function" | "Method" => format!("({})", s.name),
            _ => format!("[/{}\\]", s.name),
        };
        out.push_str(&format!("  s{}{};\n", s.id, shape));
    }
    for e in edges {
        let label = &e.kind;
        out.push_str(&format!("  s{} -->|{}| s{};\n", e.from_id, label, e.to_id));
    }
    out
}

fn render_dot(symbols: &[graphmind_db::queries::SymbolRow], edges: &[graphmind_db::queries::EdgeRow], title: &str) -> String {
    let mut out = format!("digraph \"{}\" {{\n  rankdir=LR;\n", title);
    for s in symbols {
        let shape = match s.kind.as_str() {
            "Class" | "Interface" => "box",
            "Function" | "Method" => "ellipse",
            _ => "diamond",
        };
        out.push_str(&format!("  s{} [label=\"{}\" shape={}];\n", s.id, s.name, shape));
    }
    for e in edges {
        out.push_str(&format!("  s{} -> s{} [label=\"{}\"];\n", e.from_id, e.to_id, e.kind));
    }
    out.push_str("}\n");
    out
}

// ---------------------------------------------------------------------------
// Similar symbols tool handler
// ---------------------------------------------------------------------------

fn handle_similar(args: &Value) -> Value {
    let symbol = match args.get("symbol").and_then(|v| v.as_str()) {
        Some(s) => s,
        None => return err_text("Missing required parameter: symbol"),
    };
    let limit = args.get("limit").and_then(|v| v.as_i64()).unwrap_or(10);

    with_graph(args, |gq, proj| {
        let found = gq.find_symbol(symbol);
        if found.is_empty() {
            return err_text(&format!("Symbol '{}' not found", symbol));
        }
        let results = gq.similar_symbols(found[0].id, limit);
        let mut lines = vec![format!(">> {} similar to \"{}\" [{}]:\n", results.len(), symbol, proj.slug)];
        for s in &results {
            lines.push(format!("  {} [{}] {}:{} ({:.2})", s.name, s.kind, s.file, s.line_start, s.score));
        }
        text_content(&lines.join("\n"))
    })
}

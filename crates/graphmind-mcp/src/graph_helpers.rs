use graphmind_config::ProjectConfig;
use graphmind_db::queries::GraphQueries;
use graphmind_db::schema::init_database;
use serde_json::{json, Value};

use crate::formatting::{err_text, json_text, text_content, compact_symbol_line, compact_edge_line};

pub(crate) fn with_graph<F>(args: &Value, f: F) -> Value
where
    F: FnOnce(&GraphQueries, &ProjectConfig) -> Value,
{
    let project_slug = args.get("project").and_then(|v| v.as_str());
    let proj = match resolve_project(project_slug) {
        Some(p) => p,
        None => return err_text("No project found. Register one with `graphmind init`."),
    };
    let db_path = graphmind_config::paths::graph_db_path(&proj.slug);
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

/// Resolves the project a tool call should target, mirroring the CLI's
/// `resolve_project_slug`: an explicit slug always wins; otherwise fall
/// back to the MCP server process's cwd (exact path match, then repo_id
/// auto-link for an unregistered worktree of a known repo). Deliberately
/// does NOT fall back to "the only registered project" or "the first one
/// in the map" — with several projects registered (e.g. one per worktree
/// when multiple agents each run their own `graphmind mcp` server), that
/// would silently answer from the wrong project.
pub(crate) fn resolve_project(explicit: Option<&str>) -> Option<ProjectConfig> {
    let slug = graphmind_config::resolve_project_slug(&[explicit])?;
    graphmind_config::Registry::get(&slug)
}

pub(crate) fn all_project_slugs() -> Vec<String> {
    graphmind_config::Registry::list()
        .into_iter()
        .map(|p| p.slug)
        .collect()
}

pub(crate) fn compact_edges(gq: &GraphQueries, edges: &[graphmind_db::queries::SymbolWithEdge]) -> Vec<Value> {
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

pub(crate) fn qualify_definitions(gq: &GraphQueries, symbols: &[graphmind_db::queries::SymbolRow], include_content: bool) -> Vec<Value> {
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

pub(crate) struct SymbolQueryOptions<'a> {
    pub file: Option<&'a str>,
    pub kind: Option<&'a str>,
    pub limit: usize,
    pub offset: usize,
    pub include_content: bool,
    pub format: &'a str,
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

pub(crate) fn query_symbol_filtered(gq: &GraphQueries, slug: &str, symbol: &str, opts: &SymbolQueryOptions<'_>) -> Value {
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

    let is_import = |v: &&Value| v.get("edge_kind").and_then(|k| k.as_str()) == Some("imports");
    let (import_callers_all, call_callers_all): (Vec<_>, Vec<_>) = compact_callers.iter().partition(is_import);
    let (import_callees_all, call_callees_all): (Vec<_>, Vec<_>) = compact_callees.iter().partition(is_import);

    let callers_truncated = call_callers_all.len() > limit;
    let callees_truncated = call_callees_all.len() > limit;
    let callers: Vec<_> = call_callers_all.iter().skip(offset).take(limit).collect();
    let callees: Vec<_> = call_callees_all.iter().skip(offset).take(limit).collect();
    let imported_by: Vec<_> = import_callers_all.iter().take(limit).collect();
    let imports: Vec<_> = import_callees_all.iter().take(limit).collect();

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
            let trunc = if callers_truncated { format!(" (showing {}/{}", limit, call_callers_all.len()) + ")" } else { String::new() };
            lines.push(format!("\nCallers{}:", trunc));
            for c in &callers { lines.push(compact_edge_line(c)); }
        }
        if !imported_by.is_empty() {
            let trunc = if import_callers_all.len() > limit { format!(" (showing {}/{})", limit, import_callers_all.len()) } else { String::new() };
            lines.push(format!("\nImported by{}:", trunc));
            for c in &imported_by { lines.push(compact_edge_line(c)); }
        }
        if !callees.is_empty() {
            let trunc = if callees_truncated { format!(" (showing {}/{}", limit, call_callees_all.len()) + ")" } else { String::new() };
            lines.push(format!("\nCallees{}:", trunc));
            for c in &callees { lines.push(compact_edge_line(c)); }
        }
        if !imports.is_empty() {
            let trunc = if import_callees_all.len() > limit { format!(" (showing {}/{})", limit, import_callees_all.len()) } else { String::new() };
            lines.push(format!("\nImports{}:", trunc));
            for c in &imports { lines.push(compact_edge_line(c)); }
        }
        return text_content(&lines.join("\n"));
    }

    let mut result = json!({
        "project": slug,
        "symbol": symbol,
        "definitions": definitions,
        "callers": callers,
        "callees": callees,
        "imported_by": imported_by,
        "imports": imports,
    });
    let obj = result.as_object_mut().unwrap();
    if callers_truncated {
        obj.insert("callers_truncated".into(), json!(true));
        obj.insert("total_callers".into(), json!(call_callers_all.len()));
    }
    if callees_truncated {
        obj.insert("callees_truncated".into(), json!(true));
        obj.insert("total_callees".into(), json!(call_callees_all.len()));
    }
    json_text(&result)
}

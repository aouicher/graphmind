use graphmind_db::queries::GraphQueries;
use graphmind_db::schema::init_database;
use serde_json::{json, Value};

use crate::formatting::{err_text, json_text, text_content};
use crate::graph_helpers::{
    all_project_slugs, compact_edges, qualify_definitions, query_symbol_filtered,
    resolve_project, with_graph, SymbolQueryOptions,
};

pub(crate) fn handle_query(args: &Value) -> Value {
    handle_symbol_query(args, false)
}

pub(crate) fn handle_fn(args: &Value) -> Value {
    handle_symbol_query(args, true)
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
    let include_content = args
        .get("include_content")
        .and_then(|v| v.as_bool())
        .unwrap_or(default_include_content);
    let format = args
        .get("format")
        .and_then(|v| v.as_str())
        .unwrap_or("compact");

    let opts = SymbolQueryOptions {
        file: file_filter,
        kind: kind_filter,
        limit,
        offset,
        include_content,
        format,
    };

    if project_slug.is_some() {
        return with_graph(args, |gq, proj| {
            query_symbol_filtered(gq, &proj.slug, symbol, &opts)
        });
    }

    if let Some(proj) = resolve_project(None) {
        let db_path = graphmind_config::paths::graph_db_path(&proj.slug);
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
        let db_path = graphmind_config::paths::graph_db_path(slug);
        if !db_path.exists() {
            continue;
        }
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

pub(crate) fn handle_deps(args: &Value) -> Value {
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
                let sig_part = if sig.is_empty() {
                    String::new()
                } else {
                    format!("({})", sig)
                };
                lines.push(format!(
                    "    {} {}{} :{}",
                    s.kind, s.name, sig_part, s.line_start
                ));
            }
            if all_symbols.len() > limit {
                lines.push(format!("    ... +{} more", all_symbols.len() - limit));
            }
        }

        text_content(&lines.join("\n"))
    })
}

pub(crate) fn handle_impact(args: &Value) -> Value {
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

pub(crate) fn handle_file(args: &Value) -> Value {
    let file = match args.get("file").and_then(|v| v.as_str()) {
        Some(s) => s,
        None => return err_text("Missing required parameter: file"),
    };
    with_graph(args, |_gq, proj| {
        let path = std::path::Path::new(&proj.path).join(file);
        match std::fs::read_to_string(&path) {
            Ok(content) => text_content(&format!(
                "# {file} [{slug}]\n\n```\n{content}\n```",
                slug = proj.slug
            )),
            Err(e) => err_text(&format!("Cannot read {}: {e}", path.display())),
        }
    })
}

pub(crate) fn handle_fn_impact(args: &Value) -> Value {
    let symbol = match args.get("symbol").and_then(|v| v.as_str()) {
        Some(s) => s,
        None => return err_text("Missing required parameter: symbol"),
    };
    let file_filter = args.get("file").and_then(|v| v.as_str());
    with_graph(args, |gq, _proj| {
        let callers = gq.callers_filtered(symbol, file_filter);
        let files: Vec<&str> = callers
            .iter()
            .map(|c| c.file.as_str())
            .collect::<std::collections::HashSet<_>>()
            .into_iter()
            .collect();
        json_text(&json!({
            "symbol": symbol,
            "callers": callers,
            "affected_files": files,
            "caller_count": callers.len(),
            "file_count": files.len()
        }))
    })
}

pub(crate) fn handle_map(args: &Value) -> Value {
    let limit = args.get("limit").and_then(|v| v.as_i64()).unwrap_or(20);
    with_graph(args, |gq, proj| {
        let top = gq.top_connected(limit);
        let mut lines = vec![format!(
            ">> {} top connected files [{}]:\n",
            top.len(),
            proj.slug
        )];
        for entry in &top {
            lines.push(format!("  {} ({} connections)", entry.file, entry.connections));
        }
        text_content(&lines.join("\n"))
    })
}

pub(crate) fn handle_cycles(args: &Value) -> Value {
    with_graph(args, |gq, _proj| {
        let cycles = gq.detect_cycles();
        json_text(&json!({
            "cycles": cycles,
            "count": cycles.len()
        }))
    })
}

use graphmind_db::queries::GraphQueries;
use graphmind_db::schema::init_database;
use serde_json::{json, Value};

use crate::formatting::{compact_edge_line, compact_symbol_line, err_text, json_text, text_content};
use crate::graph_helpers::{all_project_slugs, compact_edges};
use crate::search_helpers::{fuse_fts_and_semantic, load_embed_engine, remote_semantic_search, FusedResult};

pub(crate) fn handle_search(args: &Value) -> Value {
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
    let limit = args.get("limit").and_then(|v| v.as_i64()).unwrap_or(20);
    let kind_raw = args.get("kind").and_then(|v| v.as_str());
    let kind_normalized: Option<String> = kind_raw.map(|k| {
        let mut c = k.chars();
        match c.next() {
            Some(first) => first.to_uppercase().to_string() + &c.as_str().to_lowercase(),
            None => String::new(),
        }
    });
    let kind_filter = kind_normalized.as_deref();
    let include_content = args
        .get("include_content")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let format = args
        .get("format")
        .and_then(|v| v.as_str())
        .unwrap_or("compact");
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
        let db_path = graphmind_config::paths::graph_db_path(slug);
        if !db_path.exists() {
            continue;
        }
        let db_path_str = db_path.to_string_lossy().to_string();
        let conn = match init_database(&db_path_str) {
            Ok(c) => c,
            Err(_) => continue,
        };
        let gq = GraphQueries::new(&conn);
        let fts_results: Vec<_> = gq
            .search_symbols_filtered(query, limit * 3, kind_filter)
            .into_iter()
            .take(limit as usize)
            .collect();

        // Semantic search via embeddings (local engine or remote API)
        let merged = if let Some(ref engine) = embed_engine {
            let emb_db_path = graphmind_config::paths::embedding_db_path(slug);
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
                fts_results
                    .into_iter()
                    .map(|s| FusedResult::from_symbol(s, None))
                    .collect()
            }
        } else if graphmind_api_client::is_remote_embed(&graphmind_config::config::load_config()) {
            let semantic_results = remote_semantic_search(slug, raw_query, limit as usize);
            fuse_fts_and_semantic(&fts_results, &semantic_results, limit as usize)
        } else {
            fts_results
                .into_iter()
                .map(|s| FusedResult::from_symbol(s, None))
                .collect()
        };

        if !merged.is_empty() {
            let compact: Vec<Value> = merged
                .iter()
                .map(|r| {
                    let mut entry = json!({
                        "name": r.name,
                        "kind": r.kind,
                        "file": r.file,
                        "line_start": r.line_start,
                        "score": r.score,
                    });
                    let obj = entry.as_object_mut().unwrap();
                    if let Some(ref sig) = r.signature {
                        if !sig.is_empty() {
                            obj.insert("signature".into(), json!(sig));
                        }
                    }
                    if let Some(ref snippet) = r.snippet {
                        if !snippet.is_empty() {
                            obj.insert("snippet".into(), json!(snippet));
                        }
                    }
                    if include_content {
                        if let Some(ref content) = r.content {
                            obj.insert("content".into(), json!(content));
                        }
                    }
                    entry
                })
                .collect();
            all_results.push(json!({
                "project": slug,
                "symbols": compact
            }));
        }
    }

    let total_count: usize = all_results
        .iter()
        .map(|r| {
            r.get("symbols")
                .and_then(|v| v.as_array())
                .map(|a| a.len())
                .unwrap_or(0)
        })
        .sum();

    // Auto-expand: if exactly 1 result, add callers/callees (like gm_fn)
    if total_count == 1 {
        if let Some(first_project) = all_results.first() {
            let slug = first_project
                .get("project")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            if let Some(symbols) = first_project.get("symbols").and_then(|v| v.as_array()) {
                if let Some(sym) = symbols.first() {
                    let sym_name = sym.get("name").and_then(|v| v.as_str()).unwrap_or("");
                    let sym_file = sym.get("file").and_then(|v| v.as_str());
                    if !sym_name.is_empty() {
                        let db_path = graphmind_config::paths::graph_db_path(slug);
                        if let Ok(conn) = init_database(&db_path.to_string_lossy()) {
                            let gq = GraphQueries::new(&conn);
                            let callers = gq.callers_filtered(sym_name, sym_file);
                            let callees = gq.callees_filtered(sym_name, sym_file);
                            let compact_callers = compact_edges(&gq, &callers);
                            let compact_callees = compact_edges(&gq, &callees);

                            if format == "compact" {
                                let mut lines = vec![format!(
                                    ">> 1 result for \"{}\" (auto-expanded):\n",
                                    raw_query
                                )];
                                lines.push(compact_symbol_line(sym));
                                if !compact_callers.is_empty() {
                                    lines.push(format!(
                                        "\n  Callers ({}):",
                                        compact_callers.len()
                                    ));
                                    for c in compact_callers.iter().take(10) {
                                        lines.push(format!("    ← {}", compact_edge_line(c)));
                                    }
                                    if compact_callers.len() > 10 {
                                        lines.push(format!(
                                            "    ... and {} more",
                                            compact_callers.len() - 10
                                        ));
                                    }
                                }
                                if !compact_callees.is_empty() {
                                    lines.push(format!(
                                        "\n  Callees ({}):",
                                        compact_callees.len()
                                    ));
                                    for c in compact_callees.iter().take(10) {
                                        lines.push(format!("    → {}", compact_edge_line(c)));
                                    }
                                    if compact_callees.len() > 10 {
                                        lines.push(format!(
                                            "    ... and {} more",
                                            compact_callees.len() - 10
                                        ));
                                    }
                                }
                                return text_content(&lines.join("\n"));
                            } else {
                                let mut result = all_results.into_iter().next().unwrap();
                                let obj = result.as_object_mut().unwrap();
                                obj.insert(
                                    "callers".into(),
                                    json!(compact_callers.iter().take(15).collect::<Vec<_>>()),
                                );
                                obj.insert(
                                    "callees".into(),
                                    json!(compact_callees.iter().take(15).collect::<Vec<_>>()),
                                );
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
        let mut lines = vec![format!(
            ">> {} result(s) for \"{}\":\n",
            total_count, raw_query
        )];
        for r in &all_results {
            if let Some(symbols) = r.get("symbols").and_then(|v| v.as_array()) {
                for s in symbols {
                    lines.push(compact_symbol_line(s));
                }
            }
        }
        if total_count > 1 {
            lines.push(format!(
                "\n({} matches — specify file= to narrow)",
                total_count
            ));
        }
        text_content(&lines.join("\n"))
    } else {
        json_text(&json!({
            "query": raw_query,
            "results": all_results,
        }))
    }
}

pub(crate) fn handle_listeners(args: &Value) -> Value {
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
        let db_path = graphmind_config::paths::graph_db_path(slug);
        if !db_path.exists() {
            continue;
        }
        let db_path_str = db_path.to_string_lossy().to_string();
        let conn = match init_database(&db_path_str) {
            Ok(c) => c,
            Err(_) => continue,
        };
        let gq = GraphQueries::new(&conn);
        let listeners = gq.find_listeners(event);
        if !listeners.is_empty() {
            let compact: Vec<Value> = listeners
                .iter()
                .map(|s| {
                    let qualified = gq.resolve_qualified_name(s);
                    let snippet = s
                        .content
                        .as_deref()
                        .map(|c| c.lines().take(3).collect::<Vec<_>>().join("\n"));
                    let mut entry = json!({
                        "name": s.name,
                        "qualified_name": qualified,
                        "kind": s.kind,
                        "file": s.file,
                        "line_start": s.line_start,
                    });
                    let obj = entry.as_object_mut().unwrap();
                    if let Some(ref sig) = s.signature {
                        if !sig.is_empty() {
                            obj.insert("signature".into(), json!(sig));
                        }
                    }
                    if let Some(ref snip) = snippet {
                        if !snip.is_empty() {
                            obj.insert("snippet".into(), json!(snip));
                        }
                    }
                    entry
                })
                .collect();
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
                    lines.push(format!(
                        "  {name} [{kind}] {file}:{line} ({proj})\n    ({sig})"
                    ));
                }
            }
        }
    }
    if lines.len() == 1 {
        lines.push("  (none found)".to_string());
    }
    text_content(&lines.join("\n"))
}

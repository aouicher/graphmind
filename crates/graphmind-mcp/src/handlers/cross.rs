use graphmind_db::queries::GraphQueries;
use graphmind_db::schema::init_database;
use graphmind_memory::cross_links::CrossLinkStore;
use serde_json::{json, Value};
use std::path::Path;
use std::process::Command;

use crate::formatting::{err_text, json_text};
use crate::graph_helpers::{all_project_slugs, resolve_project};

pub(crate) fn handle_cross_query(args: &Value) -> Value {
    let symbol = match args.get("symbol").and_then(|v| v.as_str()) {
        Some(s) => s,
        None => return err_text("Missing required parameter: symbol"),
    };

    let slugs = all_project_slugs();
    let mut results: Vec<Value> = Vec::new();

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

pub(crate) fn handle_cross_deps(args: &Value) -> Value {
    let project = match args.get("project").and_then(|v| v.as_str()) {
        Some(p) => p,
        None => return err_text("Missing required parameter: project"),
    };

    let cl_store = CrossLinkStore::new(&graphmind_config::paths::cross_links_path());
    let links = cl_store.find_by_project(project);

    json_text(&json!({
        "project": project,
        "cross_dependencies": links,
        "count": links.len()
    }))
}

pub(crate) fn handle_cross_links(_args: &Value) -> Value {
    let cl_store = CrossLinkStore::new(&graphmind_config::paths::cross_links_path());
    let links = cl_store.list();

    json_text(&json!({
        "links": links,
        "count": links.len()
    }))
}

pub(crate) fn handle_diff_impact(args: &Value) -> Value {
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
    let db_path = graphmind_config::paths::graph_db_path(&proj.slug);
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

use graphmind_db::queries::GraphQueries;
use graphmind_db::schema::init_database;
use graphmind_memory::cross_links::CrossLinkStore;
use graphmind_memory::store::MemoryStore;
use serde_json::{json, Value};

use crate::formatting::{err_text, format_local_time, json_text, text_content};
use crate::graph_helpers::{resolve_project, with_graph};

pub(crate) fn handle_list_projects(_args: &Value) -> Value {
    let projects = graphmind_config::Registry::list();
    let mut lines = vec![format!(">> {} registered projects:\n", projects.len())];
    for p in &projects {
        let last = p.last_build.as_deref().map(format_local_time).unwrap_or_else(|| "never".to_string());
        lines.push(format!("  {} — {} (built: {})", p.slug, p.path, last));
    }
    text_content(&lines.join("\n"))
}

pub(crate) fn handle_status(args: &Value) -> Value {
    with_graph(args, |gq, proj| {
        let stats = gq.stats();
        let langs = gq.language_breakdown();

        let symbols = stats.symbols;
        let edges = stats.edges;
        let files = stats.files;

        let lang_str: String = langs
            .iter()
            .map(|l| format!("{} ({})", l.language, l.count))
            .collect::<Vec<_>>()
            .join(", ");

        let last_build = proj.last_build.as_deref().map(format_local_time).unwrap_or_else(|| "never".to_string());
        let last_build = last_build.as_str();

        let mut text = format!(
            ">> Project: {}\n  Path: {}\n  Last build: {}\n  Graph: {} symbols, {} edges, {} files\n  Languages: {}",
            proj.slug, proj.path, last_build, symbols, edges, files, lang_str
        );
        if let Some(notice) = super::update_notice() {
            text.push('\n');
            text.push_str(&notice);
        }
        text_content(&text)
    })
}

pub(crate) fn handle_context(args: &Value) -> Value {
    let project_slug = args.get("project").and_then(|v| v.as_str());
    let proj = match resolve_project(project_slug) {
        Some(p) => p,
        None => return err_text("No project found."),
    };

    // Graph stats
    let graph_info = {
        let db_path = graphmind_config::paths::graph_db_path(&proj.slug);
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
    let store = MemoryStore::new(&graphmind_config::paths::memory_dir());
    let recent_memory: Vec<_> = store.list(Some(&proj.slug)).into_iter().take(10).collect();

    // Cross links
    let cl_store = CrossLinkStore::new(&graphmind_config::paths::cross_links_path());
    let cross_links = cl_store.find_by_project(&proj.slug);

    let mut out = json!({
        "project": proj.slug,
        "path": proj.path,
        "graph": graph_info,
        "recent_memory": recent_memory,
        "cross_links": cross_links
    });
    if let Some(notice) = super::update_notice() {
        out["update_notice"] = json!(notice);
    }
    json_text(&out)
}

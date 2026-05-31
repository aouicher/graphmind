pub(crate) mod graph;
pub(crate) mod memory;
pub(crate) mod meta;
pub(crate) mod cross;
pub(crate) mod search;
pub(crate) mod analysis;
pub(crate) mod export;

use crate::formatting::err_text;
use serde_json::Value;

// ---------------------------------------------------------------------------
// Update notice (cache-only, no network)
// ---------------------------------------------------------------------------

pub fn update_notice() -> Option<String> {
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

    fn parse(v: &str) -> (u64, u64, u64) {
        let mut p = v.trim_start_matches('v').splitn(3, '.');
        let a = p.next().and_then(|s| s.parse().ok()).unwrap_or(0);
        let b = p.next().and_then(|s| s.parse().ok()).unwrap_or(0);
        let c = p.next().and_then(|s| s.parse().ok()).unwrap_or(0);
        (a, b, c)
    }

    if parse(&cache.latest_version) > parse(CURRENT) {
        let mut msg = format!(
            "⚠ graphmind update available: {} → {} — run `graphmind update`",
            CURRENT, cache.latest_version
        );
        if graphmind_config::update_crosses_breaking(CURRENT, &cache.latest_version) {
            msg.push_str(
                " — then run `graphmind build --reset --all` (graph schema changed)"
            );
        }
        Some(msg)
    } else {
        // Check if any registered project DB has a stale schema
        let stale = graphmind_config::Registry::list().iter().any(|p| {
            let db_path = graphmind_config::paths::graph_db_path(&p.slug);
            graphmind_db::schema::schema_needs_reset(db_path.to_str().unwrap_or(""))
        });
        if stale {
            Some(
                "⚠ Graph index schema is outdated. Run `graphmind build --reset --all` to reindex with current edge kinds.".to_string()
            )
        } else {
            None
        }
    }
}

// ---------------------------------------------------------------------------
// Dispatcher
// ---------------------------------------------------------------------------

pub fn dispatch_tool(name: &str, args: &Value) -> Value {
    match name {
        // Graph
        "gm_query" => graph::handle_query(args),
        "gm_fn" => graph::handle_fn(args),
        "gm_deps" => graph::handle_deps(args),
        "gm_impact" => graph::handle_impact(args),
        "gm_fn_impact" => graph::handle_fn_impact(args),
        "gm_map" => graph::handle_map(args),
        "gm_cycles" => graph::handle_cycles(args),
        "gm_file" => graph::handle_file(args),
        // Memory
        "gm_memory_search" => memory::handle_memory_search(args),
        "gm_memory_add" => memory::handle_memory_add(args),
        "gm_memory_list" => memory::handle_memory_list(args),
        "gm_session_analyze" => memory::handle_session_analyze(args),
        // Meta
        "gm_list_projects" => meta::handle_list_projects(args),
        "gm_status" => meta::handle_status(args),
        "gm_context" => meta::handle_context(args),
        // Cross
        "gm_cross_query" => cross::handle_cross_query(args),
        "gm_cross_deps" => cross::handle_cross_deps(args),
        "gm_cross_links" => cross::handle_cross_links(args),
        "gm_diff_impact" => cross::handle_diff_impact(args),
        // Search
        "gm_search" => search::handle_search(args),
        "gm_listeners" => search::handle_listeners(args),
        // Analysis
        "gm_outline" => analysis::handle_outline(args),
        "gm_who_calls_chain" => analysis::handle_who_calls_chain(args),
        "gm_dead_code" => analysis::handle_dead_code(args),
        "gm_similar" => analysis::handle_similar(args),
        // Export
        "gm_export" => export::handle_export(args),
        _ => err_text(&format!("Unknown tool: {name}")),
    }
}

use crate::handlers;
use rmcp::model::{
    CallToolRequestParams, CallToolResult, Content, Implementation, ListToolsResult,
    ServerCapabilities, ServerInfo, Tool,
};
use rmcp::{ErrorData as McpError, RoleServer, ServerHandler, ServiceExt, transport::stdio};
use serde_json::json;
use std::sync::{Arc, OnceLock};

static SESSION_NOTICE_SHOWN: OnceLock<()> = OnceLock::new();

fn take_session_notice() -> Option<String> {
    if SESSION_NOTICE_SHOWN.set(()).is_err() {
        return None; // already shown this session
    }
    handlers::update_notice()
}

#[derive(Debug, Clone)]
pub struct GraphmindServer;

fn make_tool(name: &'static str, description: &'static str, schema: serde_json::Value) -> Tool {
    let input_schema: Arc<serde_json::Map<String, serde_json::Value>> = match schema {
        serde_json::Value::Object(map) => Arc::new(map),
        _ => Arc::new(serde_json::Map::new()),
    };
    Tool::new(name, description, input_schema)
}

fn tool_defs() -> Vec<Tool> {
    vec![
        make_tool("gm_query", "Resolve a symbol to its location + direct callers/callees as compact 5-line snippets (no full source). Use when you need the call graph but not the implementation. For full source, use gm_fn. Use file= or kind= to disambiguate overloaded names.", json!({
            "type": "object",
            "properties": {
                "symbol": { "type": "string", "description": "Symbol name to find" },
                "file": { "type": "string", "description": "Filter by file path to disambiguate common symbol names" },
                "kind": { "type": "string", "description": "Filter by symbol kind: Function, Method, Class, Interface, Type" },
                "project": { "type": "string", "description": "Project slug (optional — searches all projects if omitted)" },
                "limit": { "type": "integer", "description": "Max callers/callees to return (default 50)" },
                "offset": { "type": "integer", "description": "Skip N callers/callees for pagination (default 0)" }
            },
            "required": ["symbol"]
        })),
        make_tool("gm_fn", "Returns full source code + direct callers + direct callees for ONE symbol. Use when you need to read the implementation. Returns more data than gm_query (full source vs 5-line snippets). Pass file= or kind= when the name is ambiguous.", json!({
            "type": "object",
            "properties": {
                "symbol": { "type": "string", "description": "Function name" },
                "file": { "type": "string", "description": "Filter by file path to disambiguate common symbol names" },
                "kind": { "type": "string", "description": "Filter by symbol kind: Function, Method, Class, Interface, Type" },
                "project": { "type": "string", "description": "Project slug (optional — searches all projects if omitted)" },
                "limit": { "type": "integer", "description": "Max callers/callees to return (default 50)" },
                "offset": { "type": "integer", "description": "Skip N callers/callees for pagination (default 0)" }
            },
            "required": ["symbol"]
        })),
        make_tool("gm_deps", "Returns all symbols a file imports or uses (outgoing dependencies). Use before refactoring to understand what a file depends on. Reverse direction: gm_impact (what depends on this file).", json!({
            "type": "object",
            "properties": {
                "file": { "type": "string", "description": "File path" },
                "project": { "type": "string", "description": "Project slug (optional)" },
                "limit": { "type": "integer", "description": "Max symbols to return (default 100)" }
            },
            "required": ["file"]
        })),
        make_tool("gm_impact", "Returns all files that transitively depend on a given file (incoming, reverse of gm_deps). Use before deleting or changing a file's public API to understand blast radius.", json!({
            "type": "object",
            "properties": {
                "file": { "type": "string", "description": "File path" },
                "depth": { "type": "integer", "description": "Max traversal depth (default 5)" },
                "project": { "type": "string", "description": "Project slug (optional)" }
            },
            "required": ["file"]
        })),
        make_tool("gm_fn_impact", "Returns all transitive callers of a function (full blast radius at symbol level). Use before changing a function's signature or behavior. Complements gm_impact (file-level) — this is symbol-level. Pass file= when the name is ambiguous (matches multiple symbols across the codebase).", json!({
            "type": "object",
            "properties": {
                "symbol": { "type": "string", "description": "Function name" },
                "file": { "type": "string", "description": "Filter by file path to disambiguate common symbol names" },
                "project": { "type": "string", "description": "Project slug (optional)" }
            },
            "required": ["symbol"]
        })),
        make_tool("gm_map", "Lists the most connected files in the project ranked by edge count (imports + importers). Use as the first step to understand project architecture. Returns file names with connectivity scores — not a full dependency graph.", json!({
            "type": "object",
            "properties": {
                "limit": { "type": "integer", "description": "Max results (default 20)" },
                "project": { "type": "string", "description": "Project slug (optional)" }
            }
        })),
        make_tool("gm_cycles", "Detects circular import/dependency chains between files in the project. Returns each cycle as an ordered list of files. Use when diagnosing build issues, layering violations, or before major refactors.", json!({
            "type": "object",
            "properties": {
                "project": { "type": "string", "description": "Project slug (optional)" }
            }
        })),
        make_tool("gm_memory_search", "Searches saved memory entries by keyword (decisions, patterns, conventions, bugs). Call at session start with relevant topic keywords to recall prior context. Returns ranked entries with type and tags. Use gm_memory_list to browse all entries without filtering.", json!({
            "type": "object",
            "properties": {
                "query": { "type": "string", "description": "Search query" },
                "limit": { "type": "integer", "description": "Max results (default 20)" },
                "project": { "type": "string", "description": "Project slug (optional)" }
            },
            "required": ["query"]
        })),
        make_tool("gm_memory_add", "Save a fact to persistent memory. Use proactively when decisions, patterns, conventions, preferences, or important context emerge. No confirmation needed — saves immediately.", json!({
            "type": "object",
            "properties": {
                "content": { "type": "string", "description": "The fact/decision/pattern to remember. Keep atomic: one clear fact per entry." },
                "type": { "type": "string", "description": "Memory type: decision, pattern, convention, bug, context, session" },
                "tags": { "type": "array", "items": { "type": "string" }, "description": "Tags for categorization" },
                "project": { "type": "string", "description": "Project slug (optional — for project-scoped facts)" },
                "global": { "type": "boolean", "description": "Store as global memory (default false) — use for user preferences, cross-project knowledge" },
                "priority": { "type": "boolean", "description": "Always-inject memory (shown at every session start and prompt). Use for critical conventions, active decisions, user preferences." }
            },
            "required": ["content"]
        })),
        make_tool("gm_memory_list", "Lists all memory entries for a project without filtering (paginated). Use to audit what has been saved or when you don't have a specific search keyword. For keyword-based recall, prefer gm_memory_search.", json!({
            "type": "object",
            "properties": {
                "project": { "type": "string", "description": "Project slug (optional)" },
                "limit": { "type": "integer", "description": "Max results (default 20)" }
            }
        })),
        make_tool("gm_session_analyze", "Batch-save multiple facts from a session in one call with automatic dedup. Pass an array of facts (content, type, tags, priority). Skips entries too similar to existing ones (Jaccard>0.80). Use at session checkpoints or end-of-session instead of multiple gm_memory_add calls.", json!({
            "type": "object",
            "properties": {
                "facts": {
                    "type": "array",
                    "description": "Array of facts to save",
                    "items": {
                        "type": "object",
                        "properties": {
                            "content": { "type": "string", "description": "The fact — atomic, specific, one clear statement" },
                            "type": { "type": "string", "description": "Memory type: decision, pattern, convention, bug, context" },
                            "tags": { "type": "array", "items": { "type": "string" }, "description": "Optional tags" },
                            "priority": { "type": "boolean", "description": "Inject at every session start (for critical conventions/constraints)" }
                        },
                        "required": ["content"]
                    }
                },
                "project": { "type": "string", "description": "Project slug (optional — scopes to project)" },
                "global": { "type": "boolean", "description": "Store as global memory (default false)" }
            },
            "required": ["facts"]
        })),
        make_tool("gm_list_projects", "Lists all projects registered with graphmind (name, slug, root path, last build time). Use to discover available project slugs before calling project-scoped tools. No parameters required.", json!({
            "type": "object",
            "properties": {}
        })),
        make_tool("gm_status", "Returns graph health stats for a project: symbol count, edge count, file count, last build time, and schema version. Use to verify the index is up to date before a complex analysis session.", json!({
            "type": "object",
            "properties": {
                "project": { "type": "string", "description": "Project slug (optional)" }
            }
        })),
        make_tool("gm_context", "Returns a combined session bootstrap payload: graph stats, recent memory entries, and cross-project links in one call. Use at session start when you need full project context quickly.", json!({
            "type": "object",
            "properties": {
                "project": { "type": "string", "description": "Project slug (optional)" }
            }
        })),
        make_tool("gm_cross_query", "Searches for a symbol by name across ALL registered projects. Use when you don't know which project owns a symbol, or when tracing a call that crosses project boundaries. Returns matches with project slug and file location.", json!({
            "type": "object",
            "properties": {
                "symbol": { "type": "string", "description": "Symbol name to find" }
            },
            "required": ["symbol"]
        })),
        make_tool("gm_cross_deps", "Lists all known cross-project dependency links for a specific project (what other projects it references or is referenced by). Use when analyzing inter-repo coupling or planning cross-project refactors. Requires project slug.", json!({
            "type": "object",
            "properties": {
                "project": { "type": "string", "description": "Project slug" }
            },
            "required": ["project"]
        })),
        make_tool("gm_cross_links", "Lists all cross-project dependency links across every registered project in one call. Use for a global view of inter-repo relationships. No parameters. For links scoped to one project, use gm_cross_deps instead.", json!({
            "type": "object",
            "properties": {}
        })),
        make_tool("gm_diff_impact", "Analyzes the current git diff and returns which symbols and files are affected, plus their transitive dependents. Use before committing or reviewing a PR to understand the real blast radius of staged changes.", json!({
            "type": "object",
            "properties": {
                "project": { "type": "string", "description": "Project slug (optional)" },
                "depth": { "type": "integer", "description": "Max trace depth (default 5)" }
            }
        })),
        make_tool("gm_search", "Full-text + semantic search across all symbols by name or intent keyword. Use for discovery when you don't know the exact name. Returns ranked list with file and line. When exactly 1 result matches, auto-expands with callers/callees. Use gm_fn once you know the exact name.", json!({
            "type": "object",
            "properties": {
                "query": { "type": "string", "description": "Search query (natural language or FTS5 syntax, use ; for multi-query)" },
                "limit": { "type": "integer", "description": "Max results (default 20)" },
                "kind": { "type": "string", "description": "Filter by symbol kind: function, class, method, interface, type" },
                "include_content": { "type": "boolean", "description": "Include full source code in results (default false)" },
                "project": { "type": "string", "description": "Project slug (optional)" }
            },
            "required": ["query"]
        })),
        make_tool("gm_listeners", "Finds all functions/methods registered as listeners for a named domain event. Returns qualified names (e.g. Payments::BankCardPaymentListener#buy_order_created) with file locations. Use when tracing event-driven flows or mapping side effects of an event.", json!({
            "type": "object",
            "properties": {
                "event": { "type": "string", "description": "Event name (e.g. buy_order_created, order_created)" },
                "project": { "type": "string", "description": "Project slug (optional)" }
            },
            "required": ["event"]
        })),
        make_tool("gm_outline", "Returns the hierarchical symbol structure of a file (Class > Methods, Module > Functions) with line numbers and qualified names. Use before reading or editing a file to understand its shape without loading full source. Prefer over gm_file when you only need structure.", json!({
            "type": "object",
            "properties": {
                "file": { "type": "string", "description": "File path (relative to project root)" },
                "project": { "type": "string", "description": "Project slug (optional)" }
            },
            "required": ["file"]
        })),
        make_tool("gm_who_calls_chain", "Traces transitive callers of a symbol up to N hops (BFS). Answers 'how is this function reached?' Returns the call tree from entry points down. Use gm_fn_impact for a flat list; use this when you need the path structure.", json!({
            "type": "object",
            "properties": {
                "symbol": { "type": "string", "description": "Symbol name to trace callers for" },
                "depth": { "type": "integer", "description": "Max traversal depth (default 3)" },
                "project": { "type": "string", "description": "Project slug (optional)" }
            },
            "required": ["symbol"]
        })),
        make_tool("gm_dead_code", "Finds symbols (functions/methods/classes) with zero incoming call edges — likely dead code candidates. Returns name, kind, and file location. Results are heuristic: public API symbols or test helpers may appear here even if used externally.", json!({
            "type": "object",
            "properties": {
                "kind": { "type": "string", "description": "Filter by symbol kind: Function, Method, Class (default: Function+Method)" },
                "limit": { "type": "integer", "description": "Max results (default 50)" },
                "project": { "type": "string", "description": "Project slug (optional)" }
            }
        })),
        make_tool("gm_export", "Exports a file's dependency subgraph or a symbol's call neighborhood as Mermaid or DOT graph text. Use for visualizing architecture in docs or diagrams. Provide either file= (file subgraph) or symbol= (call neighborhood). Not for querying — use gm_deps or gm_fn for that.", json!({
            "type": "object",
            "properties": {
                "file": { "type": "string", "description": "File path (exports file subgraph)" },
                "symbol": { "type": "string", "description": "Symbol name (exports neighborhood)" },
                "format": { "type": "string", "description": "Output format: mermaid (default) or dot" },
                "depth": { "type": "integer", "description": "Neighborhood depth for symbol mode (default 2)" },
                "project": { "type": "string", "description": "Project slug (optional)" }
            }
        })),
        make_tool("gm_file", "Returns the full raw source content of a file from a registered project. Use when you need to read the entire file, not just symbols. Prefer gm_outline when you only need structure, and gm_fn when you need a specific symbol.", json!({
            "type": "object",
            "properties": {
                "file": { "type": "string", "description": "File path relative to project root" },
                "project": { "type": "string", "description": "Project slug (optional)" }
            },
            "required": ["file"]
        })),
        make_tool("gm_similar", "Finds structurally similar symbols to a given one — same kind, comparable size and callee count. Use to detect duplication, find parallel implementations, or spot refactoring candidates. Returns symbols ranked by structural similarity score.", json!({
            "type": "object",
            "properties": {
                "symbol": { "type": "string", "description": "Symbol name to find similar matches for" },
                "limit": { "type": "integer", "description": "Max results (default 10)" },
                "project": { "type": "string", "description": "Project slug (optional)" }
            },
            "required": ["symbol"]
        })),
    ]
}

impl ServerHandler for GraphmindServer {
    fn get_info(&self) -> ServerInfo {
        let capabilities = ServerCapabilities::builder()
            .enable_tools()
            .build();
        ServerInfo::new(capabilities)
            .with_server_info(Implementation::new("graphmind", env!("CARGO_PKG_VERSION")))
            .with_instructions("Local-first code intelligence with persistent memory.\n\nTOOL USAGE:\n- gm_search: broad discovery — find symbols by keyword/intent. Start here when you don't know the exact name.\n- gm_fn: deep dive into ONE specific symbol — source code + callers + callees. Use when you need the call graph or full implementation. Pass file= to disambiguate common names.\n- gm_outline: file structure at a glance. Use to understand a file before reading it.\n- gm_deps: what a file imports and what imports it.\n- Do NOT chain gm_fn after every gm_search. Only use gm_fn when you actually need callers/callees for your task (debugging, refactoring, impact analysis). For simple lookups, gm_search results are sufficient.\n\nAUTO-MEMORY RULES:\n1. RECALL: At the start of every conversation or when context is needed, call gm_memory_search with relevant keywords.\n2. SAVE: When the user makes a decision, states a preference, establishes a convention, or shares important context — immediately call gm_memory_add. No confirmation needed. Use --priority for critical facts that should always be injected.\n3. SCOPE: Use project=<slug> for project-specific facts. Use global=true for cross-project knowledge.\n\nThis gives you persistent memory across sessions. Use it proactively.")
    }

    fn list_tools(
        &self,
        _request: Option<rmcp::model::PaginatedRequestParams>,
        _context: rmcp::service::RequestContext<RoleServer>,
    ) -> impl std::future::Future<Output = Result<ListToolsResult, McpError>> + Send + '_ {
        std::future::ready(Ok(ListToolsResult {
            meta: None,
            next_cursor: None,
            tools: tool_defs(),
        }))
    }

    async fn call_tool(
        &self,
        request: CallToolRequestParams,
        _context: rmcp::service::RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        let name = &request.name;
        let args = serde_json::to_value(&request.arguments).unwrap_or_default();
        let result = handlers::dispatch_tool(name, &args);

        let is_error = result.get("isError").and_then(|v| v.as_bool()).unwrap_or(false);
        let text = if let Some(content_arr) = result.get("content").and_then(|v| v.as_array()) {
            content_arr
                .iter()
                .filter_map(|c| c.get("text").and_then(|t| t.as_str()))
                .collect::<Vec<_>>()
                .join("\n")
        } else {
            serde_json::to_string_pretty(&result).unwrap_or_default()
        };

        let text = if let Some(notice) = take_session_notice() {
            format!("{notice}\n\n{text}")
        } else {
            text
        };
        let content = vec![Content::text(text)];
        if is_error {
            Ok(CallToolResult::error(content))
        } else {
            Ok(CallToolResult::success(content))
        }
    }
}

pub async fn run_mcp_server() -> Result<(), Box<dyn std::error::Error>> {
    // Refresh update cache in background so MCP-only users get update notices
    std::thread::spawn(refresh_update_cache);

    let (stdin, mut stdout) = stdio();
    let mut stdin = tokio::io::BufReader::new(stdin);

    // Some clients probe with a vendor-specific request before `initialize`
    // (Copilot CLI sends `server/discover`). rmcp's handshake accepts only
    // `ping` before `initialize` and aborts on anything else, so answer those
    // probes with -32601 here and hand rmcp a stream that starts at the first
    // message it can handle. See handshake.rs and issue #109.
    let carry = crate::handshake::answer_preinit_probes(&mut stdin, &mut stdout).await;

    // Replay the message that ended the probe phase, then the live stdin.
    let input = crate::handshake::prepend(carry, stdin);

    let service = GraphmindServer.serve((input, stdout)).await?;
    service.waiting().await?;
    Ok(())
}

fn refresh_update_cache() {
    #[derive(serde::Serialize, serde::Deserialize, Default)]
    struct UpdateCache { fetched_at: u64, latest_version: String }

    const TTL: u64 = 86400;
    const URL: &str = "https://api.github.com/repos/aouicher/graphmind/releases/latest";

    let path = graphmind_config::paths::graphmind_dir().join("update-check.json");

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    // Skip if cache is fresh
    if let Ok(raw) = std::fs::read_to_string(&path) {
        if let Ok(cache) = serde_json::from_str::<UpdateCache>(&raw) {
            if now - cache.fetched_at < TTL && !cache.latest_version.is_empty() {
                return;
            }
        }
    }

    let use_rtk = std::process::Command::new("which").arg("rtk").output()
        .map(|o| o.status.success())
        .unwrap_or(false);

    let output = if use_rtk {
        std::process::Command::new("rtk")
            .args(["proxy", "curl", "-fsSL", "--max-time", "5",
                   "-H", "Accept: application/vnd.github+json", URL])
            .output()
    } else {
        std::process::Command::new("curl")
            .args(["-fsSL", "--max-time", "5",
                   "-H", "Accept: application/vnd.github+json", URL])
            .output()
    };

    let Ok(output) = output else { return };
    if !output.status.success() { return; }

    let body = String::from_utf8_lossy(&output.stdout);
    let latest = serde_json::from_str::<serde_json::Value>(&body)
        .ok()
        .and_then(|v| v.get("tag_name").and_then(|v| v.as_str())
            .map(|s| s.trim_start_matches('v').to_string()));

    let Some(latest) = latest else { return };
    if latest.is_empty() { return; }

    let cache = UpdateCache { fetched_at: now, latest_version: latest };
    if let Ok(json) = serde_json::to_string(&cache) {
        std::fs::write(&path, json).ok();
    }
}

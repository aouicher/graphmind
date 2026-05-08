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
        make_tool("gm_query", "Find a symbol and its callers/callees. Use file and kind to disambiguate common names (e.g. create, resolve). Callers/callees return compact snippets (5 lines) instead of full source.", json!({
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
        make_tool("gm_fn", "Drill into a specific symbol: full source code + callers + callees. Use AFTER gm_search when you know the exact name, or directly when the user mentions a specific function. Gives you the call graph that gm_search doesn't.", json!({
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
        make_tool("gm_deps", "Get the dependency map for a file (what it imports/uses)", json!({
            "type": "object",
            "properties": {
                "file": { "type": "string", "description": "File path" },
                "project": { "type": "string", "description": "Project slug (optional)" },
                "limit": { "type": "integer", "description": "Max symbols to return (default 100)" }
            },
            "required": ["file"]
        })),
        make_tool("gm_impact", "Get transitive reverse dependencies (what depends on this file)", json!({
            "type": "object",
            "properties": {
                "file": { "type": "string", "description": "File path" },
                "depth": { "type": "integer", "description": "Max traversal depth (default 5)" },
                "project": { "type": "string", "description": "Project slug (optional)" }
            },
            "required": ["file"]
        })),
        make_tool("gm_fn_impact", "Get blast radius for a function (all callers)", json!({
            "type": "object",
            "properties": {
                "symbol": { "type": "string", "description": "Function name" },
                "project": { "type": "string", "description": "Project slug (optional)" }
            },
            "required": ["symbol"]
        })),
        make_tool("gm_map", "Get top connected files in the project", json!({
            "type": "object",
            "properties": {
                "limit": { "type": "integer", "description": "Max results (default 20)" },
                "project": { "type": "string", "description": "Project slug (optional)" }
            }
        })),
        make_tool("gm_cycles", "Detect circular dependencies between files", json!({
            "type": "object",
            "properties": {
                "project": { "type": "string", "description": "Project slug (optional)" }
            }
        })),
        make_tool("gm_memory_search", "Search declarative memory (decisions, patterns, conventions)", json!({
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
        make_tool("gm_memory_list", "List memory entries for a project", json!({
            "type": "object",
            "properties": {
                "project": { "type": "string", "description": "Project slug (optional)" },
                "limit": { "type": "integer", "description": "Max results (default 20)" }
            }
        })),
        make_tool("gm_list_projects", "List all registered graphmind projects", json!({
            "type": "object",
            "properties": {}
        })),
        make_tool("gm_status", "Health check — graph stats for a project", json!({
            "type": "object",
            "properties": {
                "project": { "type": "string", "description": "Project slug (optional)" }
            }
        })),
        make_tool("gm_context", "Full session context: graph stats + recent memory + cross links", json!({
            "type": "object",
            "properties": {
                "project": { "type": "string", "description": "Project slug (optional)" }
            }
        })),
        make_tool("gm_cross_query", "Search for a symbol across all projects", json!({
            "type": "object",
            "properties": {
                "symbol": { "type": "string", "description": "Symbol name to find" }
            },
            "required": ["symbol"]
        })),
        make_tool("gm_cross_deps", "Find cross-project dependencies for a project", json!({
            "type": "object",
            "properties": {
                "project": { "type": "string", "description": "Project slug" }
            },
            "required": ["project"]
        })),
        make_tool("gm_cross_links", "List all cross-project links", json!({
            "type": "object",
            "properties": {}
        })),
        make_tool("gm_diff_impact", "Analyze git diff impact on the code graph", json!({
            "type": "object",
            "properties": {
                "project": { "type": "string", "description": "Project slug (optional)" },
                "depth": { "type": "integer", "description": "Max trace depth (default 5)" }
            }
        })),
        make_tool("gm_search", "Find symbols by name or keyword. Auto-expands with callers/callees when exactly 1 result matches. For multiple results, use gm_fn to drill into a specific one.", json!({
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
        make_tool("gm_listeners", "Find all functions/methods that listen to a domain event. Returns qualified names (e.g. Payments::BankCardPaymentListener#buy_order_created).", json!({
            "type": "object",
            "properties": {
                "event": { "type": "string", "description": "Event name (e.g. buy_order_created, order_created)" },
                "project": { "type": "string", "description": "Project slug (optional)" }
            },
            "required": ["event"]
        })),
        make_tool("gm_outline", "Get hierarchical file structure (Class > Methods, Module > Functions) with qualified names. Best for understanding a file before editing.", json!({
            "type": "object",
            "properties": {
                "file": { "type": "string", "description": "File path (relative to project root)" },
                "project": { "type": "string", "description": "Project slug (optional)" }
            },
            "required": ["file"]
        })),
        make_tool("gm_who_calls_chain", "Trace transitive callers of a symbol up to N depth (BFS). Answers 'how do we reach this function?' across the call graph.", json!({
            "type": "object",
            "properties": {
                "symbol": { "type": "string", "description": "Symbol name to trace callers for" },
                "depth": { "type": "integer", "description": "Max traversal depth (default 3)" },
                "project": { "type": "string", "description": "Project slug (optional)" }
            },
            "required": ["symbol"]
        })),
        make_tool("gm_dead_code", "Find symbols (functions/methods) with no incoming edges — likely dead code candidates.", json!({
            "type": "object",
            "properties": {
                "kind": { "type": "string", "description": "Filter by symbol kind: Function, Method, Class (default: Function+Method)" },
                "limit": { "type": "integer", "description": "Max results (default 50)" },
                "project": { "type": "string", "description": "Project slug (optional)" }
            }
        })),
        make_tool("gm_export", "Export a file or symbol neighborhood as Mermaid flowchart or DOT graph text.", json!({
            "type": "object",
            "properties": {
                "file": { "type": "string", "description": "File path (exports file subgraph)" },
                "symbol": { "type": "string", "description": "Symbol name (exports neighborhood)" },
                "format": { "type": "string", "description": "Output format: mermaid (default) or dot" },
                "depth": { "type": "integer", "description": "Neighborhood depth for symbol mode (default 2)" },
                "project": { "type": "string", "description": "Project slug (optional)" }
            }
        })),
        make_tool("gm_similar", "Find structurally similar symbols (same kind, similar size and callee count). Useful for detecting duplication.", json!({
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
    let service = GraphmindServer.serve(stdio()).await?;
    service.waiting().await?;
    Ok(())
}

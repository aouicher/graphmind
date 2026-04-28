use crate::handlers;
use rmcp::model::{
    CallToolRequestParams, CallToolResult, Content, Implementation, ListToolsResult,
    ServerCapabilities, ServerInfo, Tool,
};
use rmcp::{ErrorData as McpError, RoleServer, ServerHandler, ServiceExt, transport::stdio};
use serde_json::json;
use std::sync::Arc;

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
        make_tool("gm_fn", "Get full function detail with source code, callers and callees. Use file and kind to disambiguate common names (e.g. create, resolve). Callers/callees return compact snippets (5 lines) instead of full source.", json!({
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
        make_tool("gm_memory_add", "Add a fact to memory (requires confirmation). Returns preview before saving.", json!({
            "type": "object",
            "properties": {
                "content": { "type": "string", "description": "The fact/decision/pattern to remember" },
                "type": { "type": "string", "description": "Memory type: decision, pattern, convention, bug, context, session" },
                "tags": { "type": "array", "items": { "type": "string" }, "description": "Tags" },
                "project": { "type": "string", "description": "Project slug (optional)" },
                "global": { "type": "boolean", "description": "Store as global memory (default false)" },
                "confirmed": { "type": "boolean", "description": "Set to true to confirm the write" }
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
        make_tool("gm_search", "Find symbols by name or keyword. Returns name, kind, file, signature, and a 5-line snippet. Set include_content=true for full source code, or use gm_fn for a single symbol with callers/callees.", json!({
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
            .with_instructions("Local-first code intelligence: query symbols, trace dependencies, search code, manage memory across projects.")
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

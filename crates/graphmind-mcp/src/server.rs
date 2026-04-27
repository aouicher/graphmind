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
        make_tool("gm_query", "Find a symbol and its callers/callees in the code graph", json!({
            "type": "object",
            "properties": {
                "symbol": { "type": "string", "description": "Symbol name to find" },
                "project": { "type": "string", "description": "Project slug (optional)" },
                "limit": { "type": "integer", "description": "Max callers/callees to return (default 50)" },
                "offset": { "type": "integer", "description": "Skip N callers/callees for pagination (default 0)" }
            },
            "required": ["symbol"]
        })),
        make_tool("gm_fn", "Get the full call chain for a function (callers and callees)", json!({
            "type": "object",
            "properties": {
                "symbol": { "type": "string", "description": "Function name" },
                "project": { "type": "string", "description": "Project slug (optional)" },
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
                "depth": { "type": "integer", "description": "Max traversal depth (default 3)" },
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
        make_tool("gm_memory_search", "Search memory entries by query", json!({
            "type": "object",
            "properties": {
                "query": { "type": "string", "description": "Search query" },
                "limit": { "type": "integer", "description": "Max results (default 10)" },
                "project": { "type": "string", "description": "Project slug (optional)" }
            },
            "required": ["query"]
        })),
        make_tool("gm_memory_add", "Add a memory entry", json!({
            "type": "object",
            "properties": {
                "content": { "type": "string", "description": "Memory content" },
                "type": { "type": "string", "description": "Memory type: decision, pattern, convention, bug, context, session" },
                "tags": { "type": "array", "items": { "type": "string" }, "description": "Tags" },
                "project": { "type": "string", "description": "Project slug (optional)" },
                "global": { "type": "boolean", "description": "Store as global memory (default false)" }
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
                "project": { "type": "string", "description": "Project slug (optional)" }
            }
        })),
        make_tool("gm_search", "Full-text search across symbols in one or all projects. Supports natural language queries and semicolons for multi-query.", json!({
            "type": "object",
            "properties": {
                "query": { "type": "string", "description": "Search query (natural language or FTS5 syntax, use ; for multi-query)" },
                "limit": { "type": "integer", "description": "Max results (default 10)" },
                "kind": { "type": "string", "description": "Filter by symbol kind: function, class, method, interface, type" },
                "project": { "type": "string", "description": "Project slug (optional)" }
            },
            "required": ["query"]
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

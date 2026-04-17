import { Server } from "@modelcontextprotocol/sdk/server/index.js";
import { StdioServerTransport } from "@modelcontextprotocol/sdk/server/stdio.js";
import {
	CallToolRequestSchema,
	ListToolsRequestSchema,
} from "@modelcontextprotocol/sdk/types.js";
import { registerGraphTools } from "./tools/graph-tools.js";
import { registerMemoryTools } from "./tools/memory-tools.js";
import { registerMetaTools } from "./tools/meta-tools.js";
import { registerCrossTools } from "./tools/cross-tools.js";

export interface McpServerOptions {
	transport: "stdio" | "http";
	port: number;
	projects: string[] | null;
}

export async function startMcpServer(options: McpServerOptions): Promise<void> {
	if (options.transport === "http") {
		if (options.port < 1024 || options.port > 65535) {
			throw new Error(`Invalid port: ${options.port}. Must be 1024-65535.`);
		}
		console.error(`HTTP transport not yet implemented. Use stdio (default).`);
		process.exit(1);
	}

	const server = new Server(
		{ name: "graphmind", version: "0.1.0" },
		{ capabilities: { tools: {} } },
	);

	const allTools = [
		...registerGraphTools(),
		...registerMemoryTools(),
		...registerMetaTools(),
		...registerCrossTools(),
	];

	server.setRequestHandler(ListToolsRequestSchema, async () => ({
		tools: allTools.map((t) => ({ name: t.name, description: t.description, inputSchema: t.inputSchema })),
	}));

	server.setRequestHandler(CallToolRequestSchema, async (request) => {
		const tool = allTools.find((t) => t.name === request.params.name);
		if (!tool) {
			return {
				content: [{ type: "text" as const, text: `Unknown tool: ${request.params.name}` }],
				isError: true,
			};
		}

		try {
			const result = await tool.handler(request.params.arguments ?? {}, options.projects);
			return {
				content: [{ type: "text" as const, text: typeof result === "string" ? result : JSON.stringify(result, null, 2) }],
			};
		} catch (e) {
			return {
				content: [{ type: "text" as const, text: `Error: ${(e as Error).message}` }],
				isError: true,
			};
		}
	});

	const transport = new StdioServerTransport();
	await server.connect(transport);
}

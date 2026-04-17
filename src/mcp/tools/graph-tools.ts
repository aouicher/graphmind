import { existsSync } from "node:fs";
import { GraphQueries } from "../../core/graph/queries.js";
import { initDatabase } from "../../core/graph/schema.js";
import { Registry } from "../../core/registry.js";
import { graphDbPath } from "../../utils/paths.js";

interface ToolDef {
	name: string;
	description: string;
	inputSchema: Record<string, unknown>;
	handler: (args: Record<string, unknown>, projectFilter: string[] | null) => Promise<unknown>;
}

function getQueries(project?: string): { queries: GraphQueries; slug: string } | null {
	const registry = new Registry();
	const slug = project ?? registry.findByPath(process.cwd())?.slug;
	if (!slug) return null;
	const dbPath = graphDbPath(slug);
	if (!existsSync(dbPath)) return null;
	const db = initDatabase(dbPath);
	return { queries: new GraphQueries(db), slug };
}

export function registerGraphTools(): ToolDef[] {
	return [
		{
			name: "gm_query",
			description: "Find a symbol and its connections (callers, callees)",
			inputSchema: {
				type: "object",
				properties: {
					symbol: { type: "string", description: "Symbol name to find" },
					project: { type: "string", description: "Project slug (optional)" },
				},
				required: ["symbol"],
			},
			handler: async (args) => {
				const ctx = getQueries(args.project as string | undefined);
				if (!ctx) return "No graph available. Run: graphmind build";
				const symbols = ctx.queries.findSymbol(args.symbol as string);
				const callers = ctx.queries.callers(args.symbol as string);
				const callees = ctx.queries.callees(args.symbol as string);
				return { symbols, callers, callees };
			},
		},
		{
			name: "gm_fn",
			description: "Function call chain: callers and callees for a symbol",
			inputSchema: {
				type: "object",
				properties: {
					symbol: { type: "string", description: "Function name" },
					project: { type: "string" },
				},
				required: ["symbol"],
			},
			handler: async (args) => {
				const ctx = getQueries(args.project as string | undefined);
				if (!ctx) return "No graph available.";
				return {
					symbol: ctx.queries.findSymbol(args.symbol as string),
					callers: ctx.queries.callers(args.symbol as string),
					callees: ctx.queries.callees(args.symbol as string),
				};
			},
		},
		{
			name: "gm_deps",
			description: "File-level dependency map (imports/exports)",
			inputSchema: {
				type: "object",
				properties: {
					file: { type: "string", description: "File path (relative to project root)" },
					project: { type: "string" },
				},
				required: ["file"],
			},
			handler: async (args) => {
				const ctx = getQueries(args.project as string | undefined);
				if (!ctx) return "No graph available.";
				return {
					depends_on: ctx.queries.fileDeps(args.file as string),
					depended_by: ctx.queries.fileReverseDeps(args.file as string),
				};
			},
		},
		{
			name: "gm_impact",
			description: "Transitive reverse dependency trace for a file",
			inputSchema: {
				type: "object",
				properties: {
					file: { type: "string", description: "File path" },
					project: { type: "string" },
					depth: { type: "number", description: "Max trace depth (default 5)" },
				},
				required: ["file"],
			},
			handler: async (args) => {
				const ctx = getQueries(args.project as string | undefined);
				if (!ctx) return "No graph available.";
				const depth = (args.depth as number) ?? 5;
				return ctx.queries.impact(args.file as string, depth);
			},
		},
		{
			name: "gm_fn_impact",
			description: "Blast radius: all callers of a function",
			inputSchema: {
				type: "object",
				properties: {
					symbol: { type: "string" },
					project: { type: "string" },
				},
				required: ["symbol"],
			},
			handler: async (args) => {
				const ctx = getQueries(args.project as string | undefined);
				if (!ctx) return "No graph available.";
				return ctx.queries.callers(args.symbol as string);
			},
		},
		{
			name: "gm_map",
			description: "Top N most-connected files in the project",
			inputSchema: {
				type: "object",
				properties: {
					project: { type: "string" },
					n: { type: "number", description: "Number of results (default 20)" },
				},
			},
			handler: async (args) => {
				const ctx = getQueries(args.project as string | undefined);
				if (!ctx) return "No graph available.";
				return ctx.queries.topConnected((args.n as number) ?? 20);
			},
		},
		{
			name: "gm_cycles",
			description: "Detect circular dependencies between files",
			inputSchema: {
				type: "object",
				properties: { project: { type: "string" } },
			},
			handler: async (args) => {
				const ctx = getQueries(args.project as string | undefined);
				if (!ctx) return "No graph available.";
				return ctx.queries.detectCycles();
			},
		},
	];
}

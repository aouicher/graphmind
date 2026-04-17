import { existsSync } from "node:fs";
import { isAvailable } from "../../core/embeddings/engine.js";
import { semanticSearch } from "../../core/embeddings/search.js";
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

export function registerSearchTools(): ToolDef[] {
	return [
		{
			name: "gm_search",
			description:
				"Semantic search across symbols. Supports multi-query with semicolons. Falls back to FTS if embeddings unavailable.",
			inputSchema: {
				type: "object",
				properties: {
					query: { type: "string", description: "Search query (use ; for multi-query RRF)" },
					project: { type: "string", description: "Project slug (optional)" },
					kind: { type: "string", description: "Filter by symbol kind (function, class, method)" },
					limit: { type: "number", description: "Max results (default 10)" },
				},
				required: ["query"],
			},
			handler: async (args, projectFilter) => {
				const registry = new Registry();
				const slug =
					(args.project as string) ??
					projectFilter?.[0] ??
					registry.findByPath(process.cwd())?.slug;

				if (!slug) return "Not in a registered project. Provide a project slug.";

				const dbPath = graphDbPath(slug);
				if (!existsSync(dbPath)) return `No graph for "${slug}". Run: graphmind build`;

				const limit = (args.limit as number) ?? 10;
				const kind = args.kind as string | undefined;
				const query = args.query as string;

				const available = await isAvailable();
				if (available) {
					const results = await semanticSearch(slug, query, { limit, kind });
					if (results.length === 0) return "No results. Run: graphmind embed";
					return results;
				}

				const db = initDatabase(dbPath);
				const q = new GraphQueries(db);
				const ftsQuery = query
					.split(/\s+/)
					.map((w) => `${w}*`)
					.join(" ");
				let results = q.searchSymbols(ftsQuery);
				db.close();

				if (kind) results = results.filter((r) => r.kind === kind);
				return results.slice(0, limit);
			},
		},
	];
}

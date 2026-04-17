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
				"Search across symbols and source content. Supports multi-query with semicolons. Searches all projects if no slug specified.",
			inputSchema: {
				type: "object",
				properties: {
					query: { type: "string", description: "Search query (use ; for multi-query RRF)" },
					project: {
						type: "string",
						description: "Project slug (optional — searches all if omitted)",
					},
					kind: { type: "string", description: "Filter by symbol kind (function, class, method)" },
					limit: { type: "number", description: "Max results (default 10)" },
				},
				required: ["query"],
			},
			handler: async (args, projectFilter) => {
				const registry = new Registry();
				const projects = registry.list();
				const slug =
					(args.project as string) ??
					projectFilter?.[0] ??
					registry.findByPath(process.cwd())?.slug;

				const limit = (args.limit as number) ?? 10;
				const kind = args.kind as string | undefined;
				const query = args.query as string;
				const ftsQuery = query
					.split(/\s+/)
					.map((w) => `${w}*`)
					.join(" ");

				const slugs = slug ? [slug] : projects.map((p) => p.slug);
				if (slugs.length === 0) return "No projects registered. Run: graphmind register";

				const allResults: Array<Record<string, unknown>> = [];
				for (const s of slugs) {
					const dbPath = graphDbPath(s);
					if (!existsSync(dbPath)) continue;

					const available = await isAvailable();
					if (available) {
						const results = await semanticSearch(s, query, { limit, kind });
						for (const r of results) allResults.push({ project: s, ...r });
						continue;
					}

					const db = initDatabase(dbPath);
					const q = new GraphQueries(db);
					let results = q.searchSymbols(ftsQuery);
					db.close();
					if (kind) results = results.filter((r) => r.kind === kind);
					for (const r of results) allResults.push({ project: s, ...r });
				}

				if (allResults.length === 0) return "No results found.";
				return allResults.slice(0, limit);
			},
		},
	];
}

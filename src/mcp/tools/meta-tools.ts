import { existsSync, readFileSync } from "node:fs";
import { GraphQueries } from "../../core/graph/queries.js";
import { initDatabase } from "../../core/graph/schema.js";
import { MemoryStore } from "../../core/memory/store.js";
import { Registry } from "../../core/registry.js";
import { graphDbPath, metaPath } from "../../utils/paths.js";

interface ToolDef {
	name: string;
	description: string;
	inputSchema: Record<string, unknown>;
	handler: (args: Record<string, unknown>, projectFilter: string[] | null) => Promise<unknown>;
}

export function registerMetaTools(): ToolDef[] {
	return [
		{
			name: "gm_list_projects",
			description: "List all registered projects with their status",
			inputSchema: { type: "object", properties: {} },
			handler: async () => {
				const registry = new Registry();
				return registry.list().map((p) => ({
					slug: p.slug,
					path: p.path,
					lastBuild: p.lastBuild,
					languages: p.languages,
				}));
			},
		},
		{
			name: "gm_status",
			description: "Health check: registered projects, stale graphs, missing paths",
			inputSchema: { type: "object", properties: {} },
			handler: async () => {
				const registry = new Registry();
				const projects = registry.list();
				const status = projects.map((p) => {
					const issues: string[] = [];
					if (!existsSync(p.path)) issues.push("path_missing");
					if (!p.lastBuild) issues.push("never_built");
					const dbPath = graphDbPath(p.slug);
					if (!existsSync(dbPath)) issues.push("no_graph_db");

					let stats = null;
					if (existsSync(dbPath)) {
						const db = initDatabase(dbPath);
						const q = new GraphQueries(db);
						stats = q.stats();
						db.close();
					}

					return {
						slug: p.slug,
						path: p.path,
						lastBuild: p.lastBuild,
						healthy: issues.length === 0,
						issues,
						stats,
					};
				});
				return { projectCount: projects.length, projects: status };
			},
		},
		{
			name: "gm_context",
			description: "Full context for session start: graph stats + recent memory + project info",
			inputSchema: {
				type: "object",
				properties: {
					project: { type: "string", description: "Project slug" },
				},
			},
			handler: async (args) => {
				const registry = new Registry();
				const slug = (args.project as string) ?? registry.findByPath(process.cwd())?.slug;

				if (!slug) return "Not in a registered project.";

				const project = registry.get(slug);
				if (!project) return `Project "${slug}" not found.`;

				let stats = null;
				let langs: Array<{ language: string; count: number }> = [];
				const dbPath = graphDbPath(slug);
				if (existsSync(dbPath)) {
					const db = initDatabase(dbPath);
					const q = new GraphQueries(db);
					stats = q.stats();
					langs = q.languageBreakdown();
					db.close();
				}

				let meta = null;
				const mp = metaPath(slug);
				if (existsSync(mp)) {
					meta = JSON.parse(readFileSync(mp, "utf-8"));
				}

				const memoryStore = new MemoryStore();
				const recentMemories = memoryStore.list(slug).slice(0, 10);

				return {
					project: {
						slug: project.slug,
						path: project.path,
						lastBuild: project.lastBuild,
					},
					graph: stats ? { ...stats, languages: langs } : null,
					meta,
					recentMemories,
				};
			},
		},
	];
}

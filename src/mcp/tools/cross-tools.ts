import { existsSync } from "node:fs";
import { CrossLinkStore } from "../../core/cross/links.js";
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

export function registerCrossTools(): ToolDef[] {
	return [
		{
			name: "gm_cross_query",
			description: "Search for a symbol across ALL registered projects",
			inputSchema: {
				type: "object",
				properties: {
					symbol: { type: "string", description: "Symbol name to search" },
				},
				required: ["symbol"],
			},
			handler: async (args) => {
				const registry = new Registry();
				const projects = registry.list();
				const results: Array<{
					project: string;
					name: string;
					kind: string;
					file: string;
					line: number;
				}> = [];

				for (const project of projects) {
					const dbPath = graphDbPath(project.slug);
					if (!existsSync(dbPath)) continue;
					const db = initDatabase(dbPath);
					const q = new GraphQueries(db);
					const symbols = q.findSymbol(args.symbol as string);
					db.close();
					for (const s of symbols) {
						results.push({
							project: project.slug,
							name: s.name,
							kind: s.kind,
							file: s.file,
							line: s.line_start,
						});
					}
				}
				return results.length > 0
					? results
					: `No symbol "${args.symbol}" found across ${projects.length} project(s)`;
			},
		},
		{
			name: "gm_cross_deps",
			description: "Show cross-project dependencies for a project",
			inputSchema: {
				type: "object",
				properties: {
					project: { type: "string", description: "Project slug" },
				},
				required: ["project"],
			},
			handler: async (args) => {
				const store = new CrossLinkStore();
				return store.findByProject(args.project as string);
			},
		},
		{
			name: "gm_cross_links",
			description: "List all cross-project relationships",
			inputSchema: { type: "object", properties: {} },
			handler: async () => {
				const store = new CrossLinkStore();
				return store.list();
			},
		},
		{
			name: "gm_diff_impact",
			description: "Show impact of current git changes on the project graph",
			inputSchema: {
				type: "object",
				properties: {
					project: { type: "string", description: "Project slug" },
					depth: { type: "number", description: "Max trace depth (default 5)" },
				},
			},
			handler: async (args) => {
				const { execFileSync } = await import("node:child_process");
				const registry = new Registry();
				const projects = registry.list();
				const slug =
					(args.project as string) ??
					registry.findByPath(process.cwd())?.slug ??
					(projects.length === 1 ? projects[0]?.slug : undefined);
				if (!slug) return "Not in a registered project.";

				const project = registry.get(slug);
				if (!project) return `Project "${slug}" not found.`;

				const dbPath = graphDbPath(slug);
				if (!existsSync(dbPath)) return `No graph for "${slug}". Run: graphmind build`;

				let changedFiles: string[];
				try {
					const output = execFileSync("git", ["diff", "--name-only", "HEAD"], {
						cwd: project.path,
						encoding: "utf-8",
					});
					changedFiles = output
						.split("\n")
						.map((f) => f.trim())
						.filter(Boolean);
				} catch {
					return "Failed to run git diff.";
				}

				if (changedFiles.length === 0) return { changed: [], impacted: [] };

				const db = initDatabase(dbPath);
				const q = new GraphQueries(db);
				const depth = (args.depth as number) ?? 5;
				const allImpacted = new Set<string>();

				for (const file of changedFiles) {
					for (const f of q.impact(file, depth)) {
						allImpacted.add(f);
					}
				}
				db.close();

				return {
					changed: changedFiles,
					impacted: [...allImpacted].filter((f) => !changedFiles.includes(f)),
				};
			},
		},
	];
}

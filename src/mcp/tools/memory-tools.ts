import { MemorySearch } from "../../core/memory/search.js";
import { MemoryStore } from "../../core/memory/store.js";

interface ToolDef {
	name: string;
	description: string;
	inputSchema: Record<string, unknown>;
	handler: (args: Record<string, unknown>, projectFilter: string[] | null) => Promise<unknown>;
}

export function registerMemoryTools(): ToolDef[] {
	return [
		{
			name: "gm_memory_search",
			description: "Search declarative memory (decisions, patterns, conventions)",
			inputSchema: {
				type: "object",
				properties: {
					query: { type: "string", description: "Search query" },
					project: { type: "string", description: "Scope to project" },
					limit: { type: "number", description: "Max results (default 20)" },
				},
				required: ["query"],
			},
			handler: async (args) => {
				const store = new MemoryStore();
				const search = new MemorySearch();
				const entries = store.list(args.project as string | undefined);
				return search.search(entries, args.query as string, (args.limit as number) ?? 20);
			},
		},
		{
			name: "gm_memory_add",
			description:
				"Add a fact to memory (requires confirmation). Returns the entry for review before saving.",
			inputSchema: {
				type: "object",
				properties: {
					fact: { type: "string", description: "The fact/decision/pattern to remember" },
					project: { type: "string", description: "Associate with project" },
					global: { type: "boolean", description: "Store as global memory" },
					type: {
						type: "string",
						enum: ["decision", "pattern", "convention", "bug", "context", "session"],
						description: "Entry type",
					},
					tags: {
						type: "array",
						items: { type: "string" },
						description: "Tags",
					},
					confirmed: { type: "boolean", description: "Set to true to confirm the write" },
				},
				required: ["fact"],
			},
			handler: async (args) => {
				if (!args.confirmed) {
					return {
						confirmation_required: true,
						preview: {
							fact: args.fact,
							project: args.project ?? null,
							global: args.global ?? false,
							type: args.type ?? "context",
							tags: args.tags ?? [],
						},
						message: "Call gm_memory_add again with confirmed: true to save.",
					};
				}

				const store = new MemoryStore();
				const entry = store.add(args.fact as string, {
					project: args.project as string | undefined,
					global: args.global as boolean | undefined,
					type: args.type as
						| "decision"
						| "pattern"
						| "convention"
						| "bug"
						| "context"
						| "session"
						| undefined,
					tags: args.tags as string[] | undefined,
				});
				return { saved: true, id: entry.id, created: entry.created };
			},
		},
		{
			name: "gm_memory_list",
			description: "List all memory entries, optionally scoped to a project",
			inputSchema: {
				type: "object",
				properties: {
					project: { type: "string", description: "Project slug" },
				},
			},
			handler: async (args) => {
				const store = new MemoryStore();
				return store.list(args.project as string | undefined);
			},
		},
	];
}

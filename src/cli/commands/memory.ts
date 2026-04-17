import type { Command } from "commander";
import { MemorySearch } from "../../core/memory/search.js";
import { MemoryStore } from "../../core/memory/store.js";
import { log } from "../../utils/logger.js";

export function registerMemoryCommand(program: Command): void {
	const memory = program.command("memory").description("Manage declarative memory");

	memory
		.command("add <content>")
		.description("Add a memory entry")
		.option("-p, --project <slug>", "Associate with project")
		.option("-g, --global", "Store as global memory")
		.option(
			"-t, --type <type>",
			"Entry type (decision|pattern|convention|bug|context|session)",
			"context",
		)
		.option("--tags <tags...>", "Tags for the entry")
		.action(
			(
				content: string,
				opts: { project?: string; global?: boolean; type?: string; tags?: string[] },
			) => {
				const store = new MemoryStore();
				const entry = store.add(content, {
					project: opts.project,
					global: opts.global,
					type: opts.type as "decision" | "pattern" | "convention" | "bug" | "context" | "session",
					tags: opts.tags,
				});
				log.success(`Memory saved (${entry.id.slice(0, 8)})`);
			},
		);

	memory
		.command("search <query>")
		.description("Search memory entries")
		.option("-p, --project <slug>", "Scope to project")
		.option("-n, --limit <n>", "Max results", "20")
		.action((query: string, opts: { project?: string; limit: string }) => {
			const store = new MemoryStore();
			const search = new MemorySearch();
			const entries = store.list(opts.project);
			const results = search.search(entries, query, Number.parseInt(opts.limit, 10));

			if (results.length === 0) {
				log.dim("No matching memories found.");
				return;
			}

			for (const r of results) {
				const scope = r.global ? "global" : (r.project ?? "global");
				const tags = r.tags.length > 0 ? ` [${r.tags.join(", ")}]` : "";
				console.log(`\n  ${r.id.slice(0, 8)} (${r.type}, ${scope})${tags}`);
				console.log(`  ${r.content}`);
				console.log(`  ${r.created.slice(0, 10)}`);
			}
			console.log();
		});

	memory
		.command("list")
		.description("List all memory entries")
		.option("-p, --project <slug>", "Scope to project")
		.action((opts: { project?: string }) => {
			const store = new MemoryStore();
			const entries = store.list(opts.project);

			if (entries.length === 0) {
				log.dim('No memories stored. Run: graphmind memory add "<fact>"');
				return;
			}

			for (const e of entries) {
				const scope = e.global ? "global" : (e.project ?? "global");
				console.log(
					`  ${e.id.slice(0, 8)} │ ${e.type.padEnd(10)} │ ${scope.padEnd(12)} │ ${e.content.slice(0, 60)}`,
				);
			}
			console.log(`\n  ${entries.length} entries total`);
		});

	memory
		.command("delete <id>")
		.description("Delete a memory entry")
		.option("-p, --project <slug>", "Search in project memories")
		.action((id: string, opts: { project?: string }) => {
			const store = new MemoryStore();
			const deleted = store.delete(id, opts.project);
			if (deleted) {
				log.success("Memory deleted");
			} else {
				log.error(`Memory "${id}" not found`);
				process.exitCode = 1;
			}
		});
}

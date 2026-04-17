import type { Command } from "commander";
import { Registry } from "../../core/registry.js";
import { log } from "../../utils/logger.js";
import { resolveProjectSlug } from "../resolve.js";

export function registerExcludeCommand(program: Command): void {
	const exclude = program.command("exclude").description("Manage exclude patterns for a project");

	exclude
		.command("add <pattern...>")
		.description("Add exclude patterns (e.g. grafana-data)")
		.option("--in <slug>", "Project slug")
		.action((patterns: string[], opts: { in?: string }) => {
			const resolved = resolveProjectSlug(opts.in);
			if (!resolved) {
				log.error("Not in a registered project. Use --in <slug>.");
				process.exitCode = 1;
				return;
			}

			const registry = new Registry();
			const project = registry.get(resolved);
			if (!project) {
				log.error(`Project "${resolved}" not found.`);
				process.exitCode = 1;
				return;
			}

			const current = project.exclude ?? [];
			const added: string[] = [];
			for (const p of patterns) {
				const normalized = p.endsWith("/**") ? p : `${p}/**`;
				if (!current.includes(normalized)) {
					current.push(normalized);
					added.push(normalized);
				}
			}

			if (added.length === 0) {
				log.dim("All patterns already excluded.");
				return;
			}

			registry.updateProject(resolved, { exclude: current });
			log.success(`Added to "${resolved}": ${added.join(", ")}`);
			log.dim("Run: graphmind clean && graphmind build --full");
		});

	exclude
		.command("remove <pattern...>")
		.description("Remove exclude patterns")
		.option("--in <slug>", "Project slug")
		.action((patterns: string[], opts: { in?: string }) => {
			const resolved = resolveProjectSlug(opts.in);
			if (!resolved) {
				log.error("Not in a registered project. Use --in <slug>.");
				process.exitCode = 1;
				return;
			}

			const registry = new Registry();
			const project = registry.get(resolved);
			if (!project) {
				log.error(`Project "${resolved}" not found.`);
				process.exitCode = 1;
				return;
			}

			const current = project.exclude ?? [];
			const toRemove = new Set(patterns.flatMap((p) => [p, p.endsWith("/**") ? p : `${p}/**`]));
			const filtered = current.filter((e) => !toRemove.has(e));
			const removed = current.length - filtered.length;

			if (removed === 0) {
				log.dim("No matching patterns found.");
				return;
			}

			registry.updateProject(resolved, { exclude: filtered });
			log.success(`Removed ${removed} pattern(s) from "${resolved}".`);
			log.dim("Run: graphmind clean && graphmind build --full");
		});

	exclude
		.command("list")
		.description("Show current exclude patterns")
		.option("--in <slug>", "Project slug")
		.action((opts: { in?: string }) => {
			const resolved = resolveProjectSlug(opts.in);
			if (!resolved) {
				log.error("Not in a registered project. Use --in <slug>.");
				process.exitCode = 1;
				return;
			}

			const registry = new Registry();
			const project = registry.get(resolved);
			if (!project) {
				log.error(`Project "${resolved}" not found.`);
				process.exitCode = 1;
				return;
			}

			const excludes = project.exclude ?? [];
			if (excludes.length === 0) {
				log.dim("No exclude patterns configured.");
				return;
			}

			console.log(`\n  Exclude patterns for "${resolved}":\n`);
			for (const e of excludes) {
				console.log(`    ${e}`);
			}
			console.log();
		});
}

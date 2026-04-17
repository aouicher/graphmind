import type { Command } from "commander";
import { Registry } from "../../core/registry.js";
import { loadConfig, saveConfig } from "../../utils/config.js";
import { log } from "../../utils/logger.js";
import { resolveProjectSlug } from "../resolve.js";

export function registerExcludeCommand(program: Command): void {
	const exclude = program.command("exclude").description("Manage exclude patterns for a project");

	exclude
		.command("add <pattern...>")
		.description("Add exclude patterns (e.g. grafana-data)")
		.option("--in <slug>", "Project slug")
		.option("--global", "Apply to all projects")
		.action((patterns: string[], opts: { in?: string; global?: boolean }) => {
			const normalized = patterns.map((p) => (p.endsWith("/**") ? p : `${p}/**`));

			if (opts.global) {
				const config = loadConfig();
				const current = config.globalExclude ?? [];
				const added: string[] = [];
				for (const p of normalized) {
					if (!current.includes(p)) {
						current.push(p);
						added.push(p);
					}
				}
				if (added.length === 0) {
					log.dim("All patterns already in global excludes.");
					return;
				}
				config.globalExclude = current;
				saveConfig(config);
				log.success(`Added global excludes: ${added.join(", ")}`);
				log.dim("Run: graphmind clean --all && graphmind build --all --full");
				return;
			}

			const resolved = resolveProjectSlug(opts.in);
			if (!resolved) {
				log.error("Not in a registered project. Use --in <slug> or --global.");
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
			for (const p of normalized) {
				if (!current.includes(p)) {
					current.push(p);
					added.push(p);
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
		.option("--global", "Remove from global excludes")
		.action((patterns: string[], opts: { in?: string; global?: boolean }) => {
			const toRemove = new Set(patterns.flatMap((p) => [p, p.endsWith("/**") ? p : `${p}/**`]));

			if (opts.global) {
				const config = loadConfig();
				const current = config.globalExclude ?? [];
				const filtered = current.filter((e) => !toRemove.has(e));
				const removed = current.length - filtered.length;
				if (removed === 0) {
					log.dim("No matching global patterns found.");
					return;
				}
				config.globalExclude = filtered;
				saveConfig(config);
				log.success(`Removed ${removed} global pattern(s).`);
				return;
			}

			const resolved = resolveProjectSlug(opts.in);
			if (!resolved) {
				log.error("Not in a registered project. Use --in <slug> or --global.");
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
			const filtered = current.filter((e) => !toRemove.has(e));
			const removed = current.length - filtered.length;

			if (removed === 0) {
				log.dim("No matching patterns found.");
				return;
			}

			registry.updateProject(resolved, { exclude: filtered });
			log.success(`Removed ${removed} pattern(s) from "${resolved}".`);
		});

	exclude
		.command("list")
		.description("Show current exclude patterns")
		.option("--in <slug>", "Project slug")
		.action((opts: { in?: string }) => {
			const config = loadConfig();
			const globalExclude = config.globalExclude ?? [];

			if (globalExclude.length > 0) {
				console.log("\n  Global excludes:");
				for (const e of globalExclude) {
					console.log(`    ${e}`);
				}
			}

			const resolved = resolveProjectSlug(opts.in);
			if (resolved) {
				const registry = new Registry();
				const project = registry.get(resolved);
				if (project) {
					const excludes = project.exclude ?? [];
					if (excludes.length > 0) {
						console.log(`\n  Project "${resolved}" excludes:`);
						for (const e of excludes) {
							console.log(`    ${e}`);
						}
					}
				}
			}

			if (globalExclude.length === 0 && !resolved) {
				log.dim("No exclude patterns configured.");
			}
			console.log();
		});
}

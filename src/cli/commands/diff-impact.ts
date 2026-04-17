import { execFileSync } from "node:child_process";
import { existsSync } from "node:fs";
import type { Command } from "commander";
import { GraphQueries } from "../../core/graph/queries.js";
import { initDatabase } from "../../core/graph/schema.js";
import { Registry } from "../../core/registry.js";
import { log } from "../../utils/logger.js";
import { graphDbPath } from "../../utils/paths.js";

export function registerDiffImpactCommand(program: Command): void {
	program
		.command("diff-impact")
		.description("Show impact of current git changes")
		.option("--in <slug>", "Scope to a specific project")
		.option("--staged", "Only staged changes")
		.option("--depth <n>", "Max trace depth", "5")
		.action((opts: { in?: string; staged?: boolean; depth: string }) => {
			const registry = new Registry();
			const slug = opts.in ?? registry.findByPath(process.cwd())?.slug;

			if (!slug) {
				log.error("Not in a registered project.");
				process.exitCode = 1;
				return;
			}

			const project = registry.get(slug);
			if (!project) {
				log.error(`Project "${slug}" not found.`);
				process.exitCode = 1;
				return;
			}

			const dbPath = graphDbPath(slug);
			if (!existsSync(dbPath)) {
				log.error(`No graph for "${slug}". Run: graphmind build ${slug}`);
				process.exitCode = 1;
				return;
			}

			let changedFiles: string[];
			try {
				const args = opts.staged
					? ["diff", "--staged", "--name-only"]
					: ["diff", "--name-only", "HEAD"];
				const output = execFileSync("git", args, {
					cwd: project.path,
					encoding: "utf-8",
				});
				changedFiles = output
					.split("\n")
					.map((f) => f.trim())
					.filter(Boolean);
			} catch {
				log.error("Failed to run git diff. Are you in a git repository?");
				process.exitCode = 1;
				return;
			}

			if (changedFiles.length === 0) {
				log.dim("No changes detected.");
				return;
			}

			const db = initDatabase(dbPath);
			const q = new GraphQueries(db);
			const depth = Number.parseInt(opts.depth, 10);

			const allImpacted = new Set<string>();
			for (const file of changedFiles) {
				const impacted = q.impact(file, depth);
				for (const f of impacted) {
					allImpacted.add(f);
				}
			}

			db.close();

			console.log(`\n  Changed files (${changedFiles.length}):`);
			for (const f of changedFiles) {
				console.log(`    ● ${f}`);
			}

			if (allImpacted.size > 0) {
				console.log(`\n  Impacted files (${allImpacted.size}):`);
				for (const f of allImpacted) {
					if (!changedFiles.includes(f)) {
						console.log(`    ◦ ${f}`);
					}
				}
			} else {
				console.log("\n  No transitive impact detected.");
			}
			console.log();
		});
}

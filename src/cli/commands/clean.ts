import { existsSync, rmSync } from "node:fs";
import type { Command } from "commander";
import { log } from "../../utils/logger.js";
import { graphDir, graphsDir } from "../../utils/paths.js";
import { resolveProjectSlug } from "../resolve.js";

export function registerCleanCommand(program: Command): void {
	program
		.command("clean [slug]")
		.description("Remove cached graph data (forces full rebuild)")
		.option("--all", "Clean all projects")
		.action((slug: string | undefined, opts: { all?: boolean }) => {
			if (opts.all) {
				const dir = graphsDir();
				if (existsSync(dir)) {
					rmSync(dir, { recursive: true });
					log.success("Cleaned all graph data.");
				} else {
					log.info("Nothing to clean.");
				}
				return;
			}

			const resolved = resolveProjectSlug(slug);
			if (!resolved) {
				log.error("Not in a registered project. Specify a slug or use --all.");
				process.exitCode = 1;
				return;
			}

			const dir = graphDir(resolved);
			if (existsSync(dir)) {
				rmSync(dir, { recursive: true });
				log.success(`Cleaned graph data for "${resolved}".`);
			} else {
				log.info(`No graph data found for "${resolved}".`);
			}
		});
}

import { writeFileSync } from "node:fs";
import type { Command } from "commander";
import { GraphBuilder } from "../../core/graph/builder.js";
import { startWatcher } from "../../core/graph/watcher.js";
import { Registry } from "../../core/registry.js";
import { log } from "../../utils/logger.js";
import { metaPath } from "../../utils/paths.js";
import { resolveProjectSlug } from "../resolve.js";

export function registerBuildCommand(program: Command): void {
	program
		.command("build [slug]")
		.description("Build the code graph for a project")
		.option("--all", "Build all registered projects")
		.option("--full", "Force full rebuild (ignore cache)")
		.option("--watch", "Watch mode — rebuild on file changes (debounced 2s)")
		.action(
			async (
				slug: string | undefined,
				opts: { all?: boolean; full?: boolean; watch?: boolean },
			) => {
				const registry = new Registry();

				const resolved = resolveProjectSlug(slug);
				const projects = opts.all
					? registry.list()
					: slug
						? [registry.get(slug)].filter(Boolean)
						: resolved
							? [registry.get(resolved)].filter(Boolean)
							: [];

				if (projects.length === 0) {
					log.error(
						slug
							? `Project "${slug}" not found. Run: graphmind list`
							: "Not in a registered project. Run: graphmind register",
					);
					process.exitCode = 1;
					return;
				}

				if (opts.watch) {
					const watchSlug = projects[0]?.slug;
					if (!watchSlug) {
						log.error("Watch mode requires a single project.");
						process.exitCode = 1;
						return;
					}
					startWatcher(watchSlug);
					return;
				}

				for (const project of projects) {
					if (!project) continue;
					log.info(`Building graph for "${project.slug}"...`);

					const builder = new GraphBuilder(project.slug);
					try {
						const result = await builder.build(project.path, {
							full: opts.full,
							exclude: project.exclude,
						});

						registry.updateProject(project.slug, {
							lastBuild: new Date().toISOString(),
						});

						const meta = {
							lastBuild: new Date().toISOString(),
							filesProcessed: result.filesProcessed,
							symbolsFound: result.symbolsFound,
							edgesCreated: result.edgesCreated,
							duration: result.duration,
						};
						writeFileSync(metaPath(project.slug), JSON.stringify(meta, null, 2));

						log.success(
							`${project.slug}: ${result.symbolsFound} symbols, ${result.edgesCreated} edges, ${result.filesProcessed} files (${result.skipped} cached) in ${result.duration}ms`,
						);
					} catch (e) {
						log.error(`Build failed for "${project.slug}": ${(e as Error).message}`);
						process.exitCode = 1;
					} finally {
						builder.close();
					}
				}
			},
		);
}

import { existsSync } from "node:fs";
import type { Command } from "commander";
import { Registry } from "../../core/registry.js";
import { log } from "../../utils/logger.js";

export function registerRegisterCommand(program: Command): void {
	program
		.command("register [path]")
		.description("Register a project directory")
		.option("-s, --slug <slug>", "Custom project slug")
		.option("-e, --exclude <patterns...>", "Glob patterns to exclude")
		.action((path: string | undefined, opts: { slug?: string; exclude?: string[] }) => {
			const registry = new Registry();
			const targetPath = path ?? process.cwd();

			try {
				const project = registry.register(targetPath, {
					slug: opts.slug,
					exclude: opts.exclude,
				});
				log.success(`Registered "${project.slug}" at ${project.path}`);
				log.dim("Next: graphmind build");
			} catch (e) {
				log.error((e as Error).message);
				process.exitCode = 1;
			}
		});

	program
		.command("unregister <slug>")
		.description("Unregister a project")
		.action((slug: string) => {
			const registry = new Registry();
			try {
				registry.unregister(slug);
				log.success(`Unregistered "${slug}"`);
			} catch (e) {
				log.error((e as Error).message);
				process.exitCode = 1;
			}
		});

	program
		.command("list")
		.description("List all registered projects")
		.action(() => {
			const registry = new Registry();
			const projects = registry.list();

			if (projects.length === 0) {
				log.dim("No projects registered. Run: graphmind register [path]");
				return;
			}

			for (const p of projects) {
				const status = p.lastBuild ? `built ${p.lastBuild.slice(0, 10)}` : "not built";
				console.log(`  ${p.slug}  ${p.path}  (${status})`);
			}
		});

	program
		.command("status")
		.description("Show health status of all registered projects")
		.action(() => {
			const registry = new Registry();
			const projects = registry.list();

			if (projects.length === 0) {
				log.warn("No projects registered.");
				log.dim("Fix: graphmind register [path]");
				return;
			}

			console.log(`\n  ${projects.length} project(s) registered\n`);

			for (const p of projects) {
				const pathExists = existsSync(p.path);
				const issues: string[] = [];

				if (!pathExists) issues.push("path missing");
				if (!p.lastBuild) issues.push(`never built → run: graphmind build ${p.slug}`);

				if (issues.length === 0) {
					log.success(`${p.slug}: OK (built ${p.lastBuild?.slice(0, 10)})`);
				} else {
					log.warn(`${p.slug}: ${issues.join(", ")}`);
				}
			}
			console.log();
		});
}

import type { Command } from "commander";
import { SessionLogger } from "../../core/session/logger.js";
import { buildSessionContext } from "../../core/session/context.js";
import { Registry } from "../../core/registry.js";
import { log } from "../../utils/logger.js";

export function registerSessionCommand(program: Command): void {
	const session = program.command("session").description("Session logging and context");

	session
		.command("start [slug]")
		.description("Log session start and show context")
		.action((slug: string | undefined) => {
			const registry = new Registry();
			const resolved = slug ?? registry.findByPath(process.cwd())?.slug;
			if (!resolved) {
				log.error("Not in a registered project. Specify a slug or register this directory.");
				process.exitCode = 1;
				return;
			}

			const logger = new SessionLogger();
			logger.start(resolved);
			log.success(`Session started for "${resolved}"`);

			const ctx = buildSessionContext(resolved);
			if (ctx?.graph) {
				console.log(`\n  Graph: ${ctx.graph.symbols} symbols, ${ctx.graph.edges} edges, ${ctx.graph.files} files`);
				const langs = ctx.graph.languages.map((l) => `${l.language} (${l.count})`).join(", ");
				if (langs) console.log(`  Languages: ${langs}`);
			}
			if (ctx?.recentMemories && ctx.recentMemories.length > 0) {
				console.log(`\n  Recent memories:`);
				for (const m of ctx.recentMemories.slice(0, 5)) {
					console.log(`    [${m.type}] ${m.content.slice(0, 80)}`);
				}
			}
			console.log();
		});

	session
		.command("save [message]")
		.description("Save session summary to log")
		.option("-s, --slug <slug>", "Project slug")
		.action((message: string | undefined, opts: { slug?: string }) => {
			const registry = new Registry();
			const resolved = opts.slug ?? registry.findByPath(process.cwd())?.slug;
			if (!resolved) {
				log.error("Not in a registered project.");
				process.exitCode = 1;
				return;
			}

			const logger = new SessionLogger();
			const msg = message ?? "Session ended.";
			logger.save(resolved, msg);
			log.success(`Session saved for "${resolved}"`);
		});

	session
		.command("history [slug]")
		.description("Show recent session entries")
		.option("-n, --limit <n>", "Number of entries", "10")
		.action((slug: string | undefined, opts: { limit: string }) => {
			const registry = new Registry();
			const resolved = slug ?? registry.findByPath(process.cwd())?.slug;

			const logger = new SessionLogger();
			const entries = logger.history(resolved, parseInt(opts.limit, 10));

			if (entries.length === 0) {
				log.dim("No session history found.");
				return;
			}

			for (const entry of entries) {
				console.log(entry);
				console.log();
			}
		});
}

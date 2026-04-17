import { existsSync } from "node:fs";
import type { Command } from "commander";
import { inferCrossLinks } from "../../core/cross/infer.js";
import { CrossLinkStore } from "../../core/cross/links.js";
import { GraphQueries } from "../../core/graph/queries.js";
import { initDatabase } from "../../core/graph/schema.js";
import { Registry } from "../../core/registry.js";
import { log } from "../../utils/logger.js";
import { graphDbPath } from "../../utils/paths.js";

export function registerCrossCommand(program: Command): void {
	program
		.command("cross-query <symbol>")
		.description("Search for a symbol across ALL registered projects")
		.action((symbol: string) => {
			const registry = new Registry();
			const projects = registry.list();
			let found = false;

			for (const project of projects) {
				const dbPath = graphDbPath(project.slug);
				if (!existsSync(dbPath)) continue;

				const db = initDatabase(dbPath);
				const q = new GraphQueries(db);
				const results = q.findSymbol(symbol);
				db.close();

				if (results.length > 0) {
					found = true;
					for (const r of results) {
						console.log(`  ${project.slug} │ ${r.kind} ${r.name}  ${r.file}:${r.line_start}`);
						if (r.signature) console.log(`    ${r.signature}`);
					}
				}
			}

			if (!found) {
				log.dim(`No symbol "${symbol}" found across ${projects.length} project(s)`);
			}
			console.log();
		});

	program
		.command("cross-deps <slug>")
		.description("Show which projects depend on this one")
		.action((slug: string) => {
			const store = new CrossLinkStore();
			const links = store.findByProject(slug);

			if (links.length === 0) {
				log.dim(`No cross-project links for "${slug}". Run: graphmind cross-link infer`);
				return;
			}

			console.log(`\n  Cross-project links for "${slug}":`);
			for (const link of links) {
				const dir = link.from === slug ? "→" : "←";
				const other = link.from === slug ? link.to : link.from;
				console.log(`    ${dir} ${other} (${link.type}): ${link.reason}`);
			}
			console.log();
		});

	program
		.command("cross-links")
		.description("Show all cross-project relationships")
		.action(() => {
			const store = new CrossLinkStore();
			const links = store.list();

			if (links.length === 0) {
				log.dim("No cross-project links. Run: graphmind cross-link infer");
				return;
			}

			console.log(`\n  ${links.length} cross-project link(s):\n`);
			for (const link of links) {
				const inferred = link.inferred ? " (inferred)" : "";
				console.log(`  ${link.from} → ${link.to}  [${link.type}]${inferred}`);
				console.log(`    ${link.reason}`);
				if (link.symbols.length > 0) {
					console.log(`    Symbols: ${link.symbols.join(", ")}`);
				}
			}
			console.log();
		});

	const crossLink = program.command("cross-link").description("Manage cross-project links");

	crossLink
		.command("add <from> <to>")
		.description("Add a manual cross-project link")
		.requiredOption("-r, --reason <reason>", "Why these projects are linked")
		.option("-t, --type <type>", "Link type", "depends-on")
		.option("--symbols <symbols...>", "Shared symbols")
		.action(
			(from: string, to: string, opts: { reason: string; type: string; symbols?: string[] }) => {
				const store = new CrossLinkStore();
				const link = store.add({
					from,
					to,
					type: opts.type as "depends-on" | "shares-pattern" | "extends" | "uses-types-from",
					reason: opts.reason,
					symbols: opts.symbols ?? [],
					inferred: false,
					confidence: 1.0,
				});
				log.success(`Link added: ${from} → ${to} (${link.id.slice(0, 8)})`);
			},
		);

	crossLink
		.command("infer")
		.description("Auto-detect cross-project relationships from shared symbols")
		.action(() => {
			log.info("Inferring cross-project links from shared symbols...");
			const newLinks = inferCrossLinks();
			if (newLinks.length === 0) {
				log.dim("No new cross-project relationships found.");
			} else {
				log.success(`Found ${newLinks.length} new cross-project link(s)`);
				for (const link of newLinks) {
					console.log(`  ${link.from} → ${link.to}: ${link.reason}`);
				}
			}
		});
}

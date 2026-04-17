import { existsSync } from "node:fs";
import type { Command } from "commander";
import { GraphQueries } from "../../core/graph/queries.js";
import { Registry } from "../../core/registry.js";
import { log } from "../../utils/logger.js";
import { graphDbPath } from "../../utils/paths.js";
import { initDatabase } from "../../core/graph/schema.js";

function openQueries(slug: string): GraphQueries | null {
	const dbPath = graphDbPath(slug);
	if (!existsSync(dbPath)) {
		log.error(`No graph for "${slug}". Run: graphmind build ${slug}`);
		return null;
	}
	const db = initDatabase(dbPath);
	return new GraphQueries(db);
}

function resolveProject(slug: string | undefined, inProject?: string): string | null {
	const registry = new Registry();
	if (inProject) return inProject;
	if (slug) return slug;
	const project = registry.findByPath(process.cwd());
	if (project) return project.slug;
	log.error("Not in a registered project. Specify --in <slug> or register this directory.");
	return null;
}

export function registerQueryCommand(program: Command): void {
	program
		.command("query <symbol>")
		.description("Find a symbol and its connections")
		.option("--in <slug>", "Scope to a specific project")
		.action((symbol: string, opts: { in?: string }) => {
			const slug = resolveProject(undefined, opts.in);
			if (!slug) { process.exitCode = 1; return; }

			const q = openQueries(slug);
			if (!q) { process.exitCode = 1; return; }

			const symbols = q.findSymbol(symbol);
			if (symbols.length === 0) {
				log.dim(`No symbol "${symbol}" found in ${slug}`);
				return;
			}

			for (const s of symbols) {
				console.log(`\n  ${s.kind} ${s.name}  ${s.file}:${s.line_start}`);
				if (s.signature) console.log(`  ${s.signature}`);
			}

			const callers = q.callers(symbol);
			if (callers.length > 0) {
				console.log(`\n  Called by:`);
				for (const c of callers) {
					console.log(`    ${c.name} (${c.file}:${c.line_start})`);
				}
			}

			const callees = q.callees(symbol);
			if (callees.length > 0) {
				console.log(`\n  Calls:`);
				for (const c of callees) {
					console.log(`    ${c.name} (${c.file}:${c.line_start})`);
				}
			}
			console.log();
		});

	program
		.command("fn <symbol>")
		.description("Function call chain + callers")
		.option("--in <slug>", "Scope to project")
		.action((symbol: string, opts: { in?: string }) => {
			const slug = resolveProject(undefined, opts.in);
			if (!slug) { process.exitCode = 1; return; }

			const q = openQueries(slug);
			if (!q) { process.exitCode = 1; return; }

			const symbols = q.findSymbol(symbol);
			if (symbols.length === 0) {
				log.dim(`No symbol "${symbol}" found`);
				return;
			}

			console.log(`\n  ${symbol}`);
			const callers = q.callers(symbol);
			if (callers.length > 0) {
				console.log("  ← called by:");
				for (const c of callers) console.log(`    ${c.name} (${c.file}:${c.line_start})`);
			}
			const callees = q.callees(symbol);
			if (callees.length > 0) {
				console.log("  → calls:");
				for (const c of callees) console.log(`    ${c.name} (${c.file}:${c.line_start})`);
			}
			console.log();
		});

	program
		.command("deps <file>")
		.description("File-level import/export dependency map")
		.option("--in <slug>", "Scope to project")
		.action((file: string, opts: { in?: string }) => {
			const slug = resolveProject(undefined, opts.in);
			if (!slug) { process.exitCode = 1; return; }

			const q = openQueries(slug);
			if (!q) { process.exitCode = 1; return; }

			const deps = q.fileDeps(file);
			const rdeps = q.fileReverseDeps(file);

			if (deps.length > 0) {
				console.log(`\n  ${file} depends on:`);
				for (const d of deps) console.log(`    ${d.file} (${d.kind}: ${d.count})`);
			}
			if (rdeps.length > 0) {
				console.log(`\n  Depended on by:`);
				for (const d of rdeps) console.log(`    ${d.file} (${d.kind}: ${d.count})`);
			}
			if (deps.length === 0 && rdeps.length === 0) {
				log.dim(`No dependencies found for ${file}`);
			}
			console.log();
		});

	program
		.command("impact <file>")
		.description("Transitive reverse dependency trace")
		.option("--in <slug>", "Scope to project")
		.option("--depth <n>", "Max depth", "5")
		.action((file: string, opts: { in?: string; depth: string }) => {
			const slug = resolveProject(undefined, opts.in);
			if (!slug) { process.exitCode = 1; return; }

			const q = openQueries(slug);
			if (!q) { process.exitCode = 1; return; }

			const impacted = q.impact(file, parseInt(opts.depth, 10));
			if (impacted.length === 0) {
				log.dim(`No transitive dependents found for ${file}`);
				return;
			}
			console.log(`\n  Changing ${file} may affect ${impacted.length} file(s):`);
			for (const f of impacted) console.log(`    ${f}`);
			console.log();
		});

	program
		.command("fn-impact <symbol>")
		.description("Blast radius if this function changes")
		.option("--in <slug>", "Scope to project")
		.option("--no-tests", "Exclude test files")
		.action((symbol: string, opts: { in?: string; tests?: boolean }) => {
			const slug = resolveProject(undefined, opts.in);
			if (!slug) { process.exitCode = 1; return; }

			const q = openQueries(slug);
			if (!q) { process.exitCode = 1; return; }

			const symbols = q.findSymbol(symbol);
			if (symbols.length === 0) {
				log.dim(`No symbol "${symbol}" found`);
				return;
			}

			let callers = q.callers(symbol);
			if (opts.tests === false) {
				callers = callers.filter(
					(c) => !c.file.includes(".test.") && !c.file.includes(".spec."),
				);
			}

			console.log(`\n  Blast radius for ${symbol}: ${callers.length} direct caller(s)`);
			for (const c of callers) console.log(`    ${c.name} (${c.file}:${c.line_start})`);
			console.log();
		});

	program
		.command("map [slug]")
		.description("Top N most-connected files")
		.option("-n, --limit <n>", "Number of results", "20")
		.action((slug: string | undefined, opts: { limit: string }) => {
			const resolved = resolveProject(slug);
			if (!resolved) { process.exitCode = 1; return; }

			const q = openQueries(resolved);
			if (!q) { process.exitCode = 1; return; }

			const top = q.topConnected(parseInt(opts.limit, 10));
			console.log(`\n  Top connected files in ${resolved}:`);
			for (const t of top) {
				console.log(`    ${t.connections.toString().padStart(4)} │ ${t.file}`);
			}
			console.log();
		});

	program
		.command("cycles [slug]")
		.description("Detect circular dependencies")
		.action((slug: string | undefined) => {
			const resolved = resolveProject(slug);
			if (!resolved) { process.exitCode = 1; return; }

			const q = openQueries(resolved);
			if (!q) { process.exitCode = 1; return; }

			const cycles = q.detectCycles();
			if (cycles.length === 0) {
				log.success("No circular dependencies detected");
				return;
			}

			log.warn(`${cycles.length} circular dependency pair(s) found:`);
			for (const c of cycles) {
				console.log(`    ${c.from_file} ↔ ${c.to_file}`);
			}
			console.log();
		});
}

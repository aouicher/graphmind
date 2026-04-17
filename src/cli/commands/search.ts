import { existsSync } from "node:fs";
import type { Command } from "commander";
import { embed, isAvailable } from "../../core/embeddings/engine.js";
import { semanticSearch } from "../../core/embeddings/search.js";
import { EmbeddingStore, float32ToBuffer } from "../../core/embeddings/store.js";
import { GraphQueries } from "../../core/graph/queries.js";
import { initDatabase } from "../../core/graph/schema.js";
import { Registry } from "../../core/registry.js";
import { log } from "../../utils/logger.js";
import { graphDbPath } from "../../utils/paths.js";

export function registerSearchCommand(program: Command): void {
	program
		.command("search <query>")
		.description("Semantic search across symbols (requires: graphmind embed)")
		.option("--in <slug>", "Scope to a specific project")
		.option("--kind <kind>", "Filter by symbol kind (function, class, method)")
		.option("--limit <n>", "Max results", "10")
		.action(async (query: string, opts: { in?: string; kind?: string; limit: string }) => {
			const available = await isAvailable();
			if (!available) {
				log.error(
					"Semantic search requires @xenova/transformers. Install: npm i @xenova/transformers",
				);
				log.dim("Falling back to FTS keyword search...\n");
				fallbackFtsSearch(query, opts);
				return;
			}

			const registry = new Registry();
			const projects = registry.list();
			const slug =
				opts.in ??
				registry.findByPath(process.cwd())?.slug ??
				(projects.length === 1 ? projects[0]?.slug : undefined);

			if (!slug) {
				log.error("Not in a registered project. Use --in <slug>.");
				process.exitCode = 1;
				return;
			}

			const limit = Number.parseInt(opts.limit, 10);
			const results = await semanticSearch(slug, query, { limit, kind: opts.kind });

			if (results.length === 0) {
				log.dim("No semantic results. Falling back to FTS...\n");
				fallbackFtsSearch(query, opts);
				return;
			}

			console.log(`\n  Semantic search: "${query}"\n`);
			for (const r of results) {
				const score = (r.score * 100).toFixed(1);
				console.log(`  ${score}%  ${r.symbol_kind} ${r.symbol_name}  ${r.file}`);
			}
			console.log();
		});

	program
		.command("embed [slug]")
		.description("Build local embeddings for semantic search")
		.action(async (slugArg?: string) => {
			const available = await isAvailable();
			if (!available) {
				log.error("@xenova/transformers not installed. Run: npm i @xenova/transformers");
				process.exitCode = 1;
				return;
			}

			const registry = new Registry();
			const projects = registry.list();
			const slug =
				slugArg ??
				registry.findByPath(process.cwd())?.slug ??
				(projects.length === 1 ? projects[0]?.slug : undefined);

			if (!slug) {
				log.error("Not in a registered project.");
				process.exitCode = 1;
				return;
			}

			const dbPath = graphDbPath(slug);
			if (!existsSync(dbPath)) {
				log.error(`No graph for "${slug}". Run: graphmind build`);
				process.exitCode = 1;
				return;
			}

			const db = initDatabase(dbPath);
			const symbols = db
				.prepare("SELECT name, kind, file, signature FROM symbols")
				.all() as unknown as Array<{
				name: string;
				kind: string;
				file: string;
				signature: string | null;
			}>;
			db.close();

			if (symbols.length === 0) {
				log.dim("No symbols found. Build the graph first.");
				return;
			}

			log.info(`Embedding ${symbols.length} symbols for "${slug}"...`);

			const store = new EmbeddingStore(slug);
			store.clear();

			const batchSize = 50;
			let processed = 0;

			for (let i = 0; i < symbols.length; i += batchSize) {
				const batch = symbols.slice(i, i + batchSize);
				const rows = [];

				for (const sym of batch) {
					const text = sym.signature
						? `${sym.kind} ${sym.name} ${sym.signature} in ${sym.file}`
						: `${sym.kind} ${sym.name} in ${sym.file}`;

					const vec = await embed(text);
					if (!vec) continue;

					rows.push({
						symbol_name: sym.name,
						symbol_kind: sym.kind,
						file: sym.file,
						text,
						embedding: float32ToBuffer(vec),
					});
				}

				store.insertBatch(rows);
				processed += batch.length;
				process.stdout.write(`\r  ${processed}/${symbols.length} symbols embedded`);
			}

			store.close();
			console.log("\n");
			log.success(`Embedded ${processed} symbols for "${slug}".`);
		});
}

function fallbackFtsSearch(
	query: string,
	opts: { in?: string; kind?: string; limit: string },
): void {
	const registry = new Registry();
	const projects = registry.list();
	const slug =
		opts.in ??
		registry.findByPath(process.cwd())?.slug ??
		(projects.length === 1 ? projects[0]?.slug : undefined);

	if (!slug) {
		log.error("Not in a registered project. Use --in <slug>.");
		process.exitCode = 1;
		return;
	}

	const dbPath = graphDbPath(slug);
	if (!existsSync(dbPath)) {
		log.error(`No graph for "${slug}". Run: graphmind build`);
		process.exitCode = 1;
		return;
	}

	const db = initDatabase(dbPath);
	const q = new GraphQueries(db);
	const ftsQuery = query
		.split(/\s+/)
		.map((w) => `${w}*`)
		.join(" ");
	const results = q.searchSymbols(ftsQuery);
	db.close();

	const limit = Number.parseInt(opts.limit, 10);
	const filtered = opts.kind ? results.filter((r) => r.kind === opts.kind) : results;
	const limited = filtered.slice(0, limit);

	if (limited.length === 0) {
		log.dim("No results found.");
		return;
	}

	console.log(`\n  FTS search: "${query}"\n`);
	for (const r of limited) {
		console.log(`  ${r.kind} ${r.name}  ${r.file}:${r.line_start}`);
		if (r.signature) console.log(`    ${r.signature}`);
	}
	console.log();
}

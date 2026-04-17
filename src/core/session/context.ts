import { existsSync, readFileSync } from "node:fs";
import { GraphQueries } from "../graph/queries.js";
import { initDatabase } from "../graph/schema.js";
import { MemoryStore } from "../memory/store.js";
import { Registry } from "../registry.js";
import { graphDbPath, metaPath } from "../../utils/paths.js";

export interface SessionContext {
	project: {
		slug: string;
		path: string;
		lastBuild: string | null;
	};
	graph: {
		symbols: number;
		edges: number;
		files: number;
		languages: Array<{ language: string; count: number }>;
	} | null;
	recentMemories: Array<{ content: string; type: string; created: string }>;
	meta: Record<string, unknown> | null;
}

export function buildSessionContext(slug: string): SessionContext | null {
	const registry = new Registry();
	const project = registry.get(slug);
	if (!project) return null;

	let graph: SessionContext["graph"] = null;
	const dbPath = graphDbPath(slug);
	if (existsSync(dbPath)) {
		const db = initDatabase(dbPath);
		const q = new GraphQueries(db);
		const stats = q.stats();
		const langs = q.languageBreakdown();
		db.close();
		graph = { ...stats, languages: langs };
	}

	let meta: Record<string, unknown> | null = null;
	const mp = metaPath(slug);
	if (existsSync(mp)) {
		meta = JSON.parse(readFileSync(mp, "utf-8"));
	}

	const memoryStore = new MemoryStore();
	const recentMemories = memoryStore
		.list(slug)
		.slice(0, 10)
		.map((m) => ({ content: m.content, type: m.type, created: m.created }));

	return {
		project: { slug: project.slug, path: project.path, lastBuild: project.lastBuild },
		graph,
		recentMemories,
		meta,
	};
}

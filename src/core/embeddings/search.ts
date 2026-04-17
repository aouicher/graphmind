import { embed } from "./engine.js";
import { type EmbeddingRow, EmbeddingStore, bufferToFloat32 } from "./store.js";

export interface SearchResult {
	symbol_name: string;
	symbol_kind: string;
	file: string;
	text: string;
	score: number;
}

function cosineSimilarity(a: Float32Array, b: Float32Array): number {
	let dot = 0;
	let normA = 0;
	let normB = 0;
	for (let i = 0; i < a.length; i++) {
		const ai = a[i] ?? 0;
		const bi = b[i] ?? 0;
		dot += ai * bi;
		normA += ai * ai;
		normB += bi * bi;
	}
	const denom = Math.sqrt(normA) * Math.sqrt(normB);
	return denom === 0 ? 0 : dot / denom;
}

async function searchSingle(
	queryVec: Float32Array,
	rows: EmbeddingRow[],
	limit: number,
): Promise<SearchResult[]> {
	const scored = rows.map((row) => ({
		symbol_name: row.symbol_name,
		symbol_kind: row.symbol_kind,
		file: row.file,
		text: row.text,
		score: cosineSimilarity(queryVec, bufferToFloat32(row.embedding)),
	}));

	scored.sort((a, b) => b.score - a.score);
	return scored.slice(0, limit);
}

// Reciprocal Rank Fusion for multi-query
function rrfMerge(rankings: SearchResult[][], k = 60): SearchResult[] {
	const scores = new Map<string, { result: SearchResult; score: number }>();

	for (const ranking of rankings) {
		for (let i = 0; i < ranking.length; i++) {
			const r = ranking[i];
			if (!r) continue;
			const key = `${r.file}:${r.symbol_name}`;
			const existing = scores.get(key);
			const rrfScore = 1 / (k + i + 1);
			if (existing) {
				existing.score += rrfScore;
			} else {
				scores.set(key, { result: r, score: rrfScore });
			}
		}
	}

	return [...scores.values()]
		.sort((a, b) => b.score - a.score)
		.map((s) => ({ ...s.result, score: s.score }));
}

export async function semanticSearch(
	slug: string,
	query: string,
	options?: { limit?: number; kind?: string },
): Promise<SearchResult[]> {
	const limit = options?.limit ?? 20;
	const queries = query
		.split(";")
		.map((q) => q.trim())
		.filter(Boolean);

	const store = new EmbeddingStore(slug);
	let rows = store.all();
	store.close();

	if (options?.kind) {
		rows = rows.filter((r) => r.symbol_kind === options.kind);
	}

	if (rows.length === 0) return [];

	if (queries.length === 1) {
		const queryVec = await embed(queries[0] ?? "");
		if (!queryVec) return [];
		return searchSingle(queryVec, rows, limit);
	}

	// Multi-query with RRF
	const rankings: SearchResult[][] = [];
	for (const q of queries) {
		const queryVec = await embed(q);
		if (!queryVec) continue;
		rankings.push(await searchSingle(queryVec, rows, limit * 2));
	}

	return rrfMerge(rankings).slice(0, limit);
}

import type { DatabaseSync } from "node:sqlite";

export interface SymbolRow {
	id: number;
	name: string;
	kind: string;
	file: string;
	line_start: number;
	line_end: number;
	signature: string | null;
	doc: string | null;
	content: string | null;
}

export interface EdgeRow {
	id: number;
	from_id: number;
	to_id: number;
	kind: string;
	confidence: number;
	file: string | null;
}

export class GraphQueries {
	constructor(private db: DatabaseSync) {}

	findSymbol(name: string): SymbolRow[] {
		return this.db
			.prepare("SELECT * FROM symbols WHERE name = ?")
			.all(name) as unknown as SymbolRow[];
	}

	searchSymbols(query: string, limit = 20): SymbolRow[] {
		return this.db
			.prepare(
				`SELECT s.* FROM symbols_fts f
				 JOIN symbols s ON s.id = f.rowid
				 WHERE symbols_fts MATCH ?
				 ORDER BY bm25(symbols_fts, 10.0, 5.0, 3.0, 1.0)
				 LIMIT ?`,
			)
			.all(query, limit) as unknown as SymbolRow[];
	}

	callers(symbolName: string): Array<SymbolRow & { edge_kind: string }> {
		return this.db
			.prepare(
				`SELECT s.*, e.kind as edge_kind FROM edges e
				 JOIN symbols s ON s.id = e.from_id
				 JOIN symbols target ON target.id = e.to_id
				 WHERE target.name = ?`,
			)
			.all(symbolName) as unknown as Array<SymbolRow & { edge_kind: string }>;
	}

	callees(symbolName: string): Array<SymbolRow & { edge_kind: string }> {
		return this.db
			.prepare(
				`SELECT s.*, e.kind as edge_kind FROM edges e
				 JOIN symbols s ON s.id = e.to_id
				 JOIN symbols source ON source.id = e.from_id
				 WHERE source.name = ?`,
			)
			.all(symbolName) as unknown as Array<SymbolRow & { edge_kind: string }>;
	}

	fileDeps(filePath: string): Array<{ file: string; kind: string; count: number }> {
		return this.db
			.prepare(
				`SELECT target_s.file as file, e.kind, COUNT(*) as count
				 FROM edges e
				 JOIN symbols source_s ON source_s.id = e.from_id
				 JOIN symbols target_s ON target_s.id = e.to_id
				 WHERE source_s.file = ? AND target_s.file != ?
				 GROUP BY target_s.file, e.kind
				 ORDER BY count DESC`,
			)
			.all(filePath, filePath) as unknown as Array<{ file: string; kind: string; count: number }>;
	}

	fileReverseDeps(filePath: string): Array<{ file: string; kind: string; count: number }> {
		return this.db
			.prepare(
				`SELECT source_s.file as file, e.kind, COUNT(*) as count
				 FROM edges e
				 JOIN symbols source_s ON source_s.id = e.from_id
				 JOIN symbols target_s ON target_s.id = e.to_id
				 WHERE target_s.file = ? AND source_s.file != ?
				 GROUP BY source_s.file, e.kind
				 ORDER BY count DESC`,
			)
			.all(filePath, filePath) as unknown as Array<{ file: string; kind: string; count: number }>;
	}

	impact(filePath: string, maxDepth = 5): string[] {
		const visited = new Set<string>([filePath]);
		const queue = [filePath];
		let depth = 0;

		while (queue.length > 0 && depth < maxDepth) {
			const batch = [...queue];
			queue.length = 0;
			for (const file of batch) {
				const deps = this.fileReverseDeps(file);
				for (const dep of deps) {
					if (!visited.has(dep.file)) {
						visited.add(dep.file);
						queue.push(dep.file);
					}
				}
			}
			depth++;
		}

		visited.delete(filePath);
		return [...visited];
	}

	symbolsInFile(filePath: string): SymbolRow[] {
		return this.db
			.prepare("SELECT * FROM symbols WHERE file = ?")
			.all(filePath) as unknown as SymbolRow[];
	}

	topConnected(limit = 20): Array<{ file: string; connections: number }> {
		return this.db
			.prepare(
				`SELECT s.file, COUNT(DISTINCT e.id) as connections
				 FROM symbols s
				 LEFT JOIN edges e ON e.from_id = s.id OR e.to_id = s.id
				 GROUP BY s.file
				 ORDER BY connections DESC
				 LIMIT ?`,
			)
			.all(limit) as unknown as Array<{ file: string; connections: number }>;
	}

	detectCycles(): Array<{ from_file: string; to_file: string }> {
		return this.db
			.prepare(
				`SELECT DISTINCT s1.file as from_file, s2.file as to_file
				 FROM edges e1
				 JOIN symbols s1 ON s1.id = e1.from_id
				 JOIN symbols s2 ON s2.id = e1.to_id
				 WHERE s1.file != s2.file
				 AND EXISTS (
				   SELECT 1 FROM edges e2
				   JOIN symbols s3 ON s3.id = e2.from_id
				   JOIN symbols s4 ON s4.id = e2.to_id
				   WHERE s3.file = s2.file AND s4.file = s1.file
				 )`,
			)
			.all() as unknown as Array<{ from_file: string; to_file: string }>;
	}

	stats(): { symbols: number; edges: number; files: number } {
		const symbols = (
			this.db.prepare("SELECT COUNT(*) as c FROM symbols").get() as unknown as { c: number }
		).c;
		const edges = (
			this.db.prepare("SELECT COUNT(*) as c FROM edges").get() as unknown as { c: number }
		).c;
		const files = (
			this.db.prepare("SELECT COUNT(*) as c FROM files").get() as unknown as { c: number }
		).c;
		return { symbols, edges, files };
	}

	languageBreakdown(): Array<{ language: string; count: number }> {
		return this.db
			.prepare(
				"SELECT language, COUNT(*) as count FROM files WHERE language IS NOT NULL GROUP BY language ORDER BY count DESC",
			)
			.all() as unknown as Array<{ language: string; count: number }>;
	}
}

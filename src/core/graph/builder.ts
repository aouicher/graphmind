import { existsSync, mkdirSync, readFileSync, readdirSync, statSync } from "node:fs";
import { extname, join, relative } from "node:path";
import type Database from "better-sqlite3";
import { cacheDirPath, graphDbPath } from "../../utils/paths.js";
import { FileHashCache } from "./cache.js";
import { initDatabase } from "./schema.js";

const LANGUAGE_MAP: Record<string, string> = {
	".ts": "typescript",
	".tsx": "typescript",
	".js": "javascript",
	".jsx": "javascript",
	".mjs": "javascript",
};

const DEFAULT_EXCLUDE = [
	"node_modules",
	"dist",
	"build",
	".git",
	".next",
	"coverage",
	"__pycache__",
	".venv",
];

interface ParsedSymbol {
	name: string;
	kind: string;
	line_start: number;
	line_end: number;
	signature?: string;
	doc?: string;
}

interface ParsedCallSite {
	caller: string;
	callee: string;
	line: number;
}

interface ParsedImport {
	source: string;
	specifiers: string[];
	line: number;
	is_default: boolean;
	from_file: string;
}

interface ParsedFile {
	path: string;
	language: string;
	symbols: ParsedSymbol[];
	call_sites: ParsedCallSite[];
	imports: ParsedImport[];
}

export interface BuildResult {
	filesProcessed: number;
	symbolsFound: number;
	edgesCreated: number;
	skipped: number;
	duration: number;
}

export class GraphBuilder {
	private db: Database.Database;
	private cache: FileHashCache;
	private nativeParser: NativeParser | null = null;

	constructor(slug: string) {
		const dbPath = graphDbPath(slug);
		mkdirSync(join(dbPath, ".."), { recursive: true });
		this.db = initDatabase(dbPath);
		this.cache = new FileHashCache(cacheDirPath(slug));
	}

	async build(
		projectPath: string,
		options?: { full?: boolean; exclude?: string[] },
	): Promise<BuildResult> {
		const start = Date.now();
		const exclude = options?.exclude ?? DEFAULT_EXCLUDE;
		const files = this.collectFiles(projectPath, exclude);

		let processed = 0;
		let skipped = 0;
		let totalSymbols = 0;
		let totalEdges = 0;

		const parser = await this.getParser();

		const insertFile = this.db.prepare(
			"INSERT OR REPLACE INTO files (path, language, hash, last_parsed) VALUES (?, ?, ?, ?)",
		);
		const insertSymbol = this.db.prepare(
			"INSERT INTO symbols (name, kind, file, line_start, line_end, signature, doc) VALUES (?, ?, ?, ?, ?, ?, ?)",
		);
		const insertEdge = this.db.prepare(
			"INSERT INTO edges (from_id, to_id, kind, file) VALUES (?, ?, ?, ?)",
		);
		const deleteFileSymbols = this.db.prepare("DELETE FROM symbols WHERE file = ?");
		const deleteFileEdges = this.db.prepare(
			`DELETE FROM edges WHERE from_id IN (SELECT id FROM symbols WHERE file = ?)
			 OR to_id IN (SELECT id FROM symbols WHERE file = ?)`,
		);

		const processFile = this.db.transaction((file: { path: string; content: string; language: string; relPath: string }) => {
			deleteFileEdges.run(file.relPath, file.relPath);
			deleteFileSymbols.run(file.relPath);

			let parsed: ParsedFile;
			try {
				parsed = parser.parseFile(file.path, file.content, file.language);
			} catch {
				return { symbols: 0, edges: 0 };
			}

			const now = Math.floor(Date.now() / 1000);
			insertFile.run(file.relPath, file.language, "", now);

			const symbolIds = new Map<string, number>();
			for (const sym of parsed.symbols) {
				const info = insertSymbol.run(
					sym.name,
					sym.kind,
					file.relPath,
					sym.line_start,
					sym.line_end,
					sym.signature ?? null,
					sym.doc ?? null,
				);
				symbolIds.set(sym.name, Number(info.lastInsertRowid));
			}

			let edges = 0;
			for (const cs of parsed.call_sites) {
				const fromId = symbolIds.get(cs.caller);
				const toId = symbolIds.get(cs.callee);
				if (fromId && toId) {
					insertEdge.run(fromId, toId, "calls", file.relPath);
					edges++;
				}
			}

			for (const imp of parsed.imports) {
				for (const spec of imp.specifiers) {
					const cleanSpec = spec.replace(/^\* as /, "");
					const toId = symbolIds.get(cleanSpec);
					if (toId) {
						const fromSymbols = this.db
							.prepare("SELECT id FROM symbols WHERE file = ? LIMIT 1")
							.get(file.relPath) as { id: number } | undefined;
						if (fromSymbols) {
							insertEdge.run(fromSymbols.id, toId, "imports", file.relPath);
							edges++;
						}
					}
				}
			}

			return { symbols: parsed.symbols.length, edges };
		});

		for (const file of files) {
			const content = readFileSync(file.fullPath, "utf-8");
			const relPath = relative(projectPath, file.fullPath);

			if (!options?.full && !this.cache.hasChanged(relPath, content)) {
				skipped++;
				continue;
			}

			const result = processFile({
				path: file.fullPath,
				content,
				language: file.language,
				relPath,
			});
			totalSymbols += result.symbols;
			totalEdges += result.edges;
			processed++;

			this.cache.update(relPath, content);
		}

		this.cache.save();

		return {
			filesProcessed: processed,
			symbolsFound: totalSymbols,
			edgesCreated: totalEdges,
			skipped,
			duration: Date.now() - start,
		};
	}

	getDatabase(): Database.Database {
		return this.db;
	}

	close(): void {
		this.db.close();
	}

	private collectFiles(
		dir: string,
		exclude: string[],
	): Array<{ fullPath: string; language: string }> {
		const files: Array<{ fullPath: string; language: string }> = [];
		this.walkDir(dir, exclude, files);
		return files;
	}

	private walkDir(
		dir: string,
		exclude: string[],
		files: Array<{ fullPath: string; language: string }>,
	): void {
		if (!existsSync(dir)) return;
		const entries = readdirSync(dir);

		for (const entry of entries) {
			if (exclude.includes(entry)) continue;
			if (entry.startsWith(".")) continue;

			const fullPath = join(dir, entry);
			const stat = statSync(fullPath);

			if (stat.isDirectory()) {
				this.walkDir(fullPath, exclude, files);
			} else {
				const ext = extname(entry);
				const language = LANGUAGE_MAP[ext];
				if (language) {
					files.push({ fullPath, language });
				}
			}
		}
	}

	private async getParser(): Promise<NativeParser> {
		if (this.nativeParser) return this.nativeParser;

		try {
			const native = await import("../../native/graphmind-core.js");
			this.nativeParser = {
				parseFile: (path: string, source: string, language: string) =>
					native.parseFile(path, source, language),
			};
		} catch {
			this.nativeParser = new FallbackParser();
		}
		return this.nativeParser;
	}
}

interface NativeParser {
	parseFile(path: string, source: string, language: string): ParsedFile;
}

class FallbackParser implements NativeParser {
	parseFile(path: string, _source: string, language: string): ParsedFile {
		return {
			path,
			language,
			symbols: [],
			call_sites: [],
			imports: [],
		};
	}
}

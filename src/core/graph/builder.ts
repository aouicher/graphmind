import { existsSync, mkdirSync, readFileSync, readdirSync, statSync } from "node:fs";
import { dirname, extname, join, relative } from "node:path";
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
	".py": "python",
	".go": "go",
	".rs": "rust",
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
	lineStart: number;
	lineEnd: number;
	signature?: string | null;
	doc?: string | null;
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
	isDefault: boolean;
	fromFile: string;
}

interface ParsedFile {
	path: string;
	language: string;
	symbols: ParsedSymbol[];
	callSites: ParsedCallSite[];
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

		const pendingImports: Array<{ relPath: string; imports: ParsedImport[] }> = [];
		const pendingCalls: Array<{ relPath: string; callSites: ParsedCallSite[] }> = [];

		// Pass 1: Parse all files, insert symbols
		for (const file of files) {
			const content = readFileSync(file.fullPath, "utf-8");
			const relPath = relative(projectPath, file.fullPath);

			if (!options?.full && !this.cache.hasChanged(relPath, content)) {
				skipped++;
				continue;
			}

			deleteFileEdges.run(relPath, relPath);
			deleteFileSymbols.run(relPath);

			let parsed: ParsedFile;
			try {
				parsed = parser.parseFile(file.fullPath, content, file.language);
			} catch {
				continue;
			}

			const now = Math.floor(Date.now() / 1000);
			insertFile.run(relPath, file.language, "", now);

			for (const sym of parsed.symbols) {
				insertSymbol.run(
					sym.name,
					sym.kind,
					relPath,
					sym.lineStart,
					sym.lineEnd,
					sym.signature ?? null,
					sym.doc ?? null,
				);
				totalSymbols++;
			}

			if (parsed.callSites.length > 0) {
				pendingCalls.push({ relPath, callSites: parsed.callSites });
			}
			if (parsed.imports.length > 0) {
				pendingImports.push({ relPath, imports: parsed.imports });
			}

			processed++;
			this.cache.update(relPath, content);
		}

		// Pass 2: Resolve cross-file edges
		const findSymbol = this.db.prepare("SELECT id, file FROM symbols WHERE name = ?");
		const findSymbolInFile = this.db.prepare("SELECT id FROM symbols WHERE name = ? AND file = ?");
		const findFirstSymbolInFile = this.db.prepare("SELECT id FROM symbols WHERE file = ? LIMIT 1");

		const resolveEdges = this.db.transaction(() => {
			// Resolve call sites: look up callee across all files
			for (const { relPath, callSites } of pendingCalls) {
				for (const cs of callSites) {
					const callerRow = findSymbolInFile.get(cs.caller, relPath) as { id: number } | undefined;
					if (!callerRow) continue;

					// Try same file first, then any file
					let calleeRow = findSymbolInFile.get(cs.callee, relPath) as { id: number } | undefined;
					if (!calleeRow) {
						calleeRow = findSymbol.get(cs.callee) as { id: number; file: string } | undefined;
					}
					if (calleeRow) {
						insertEdge.run(callerRow.id, calleeRow.id, "calls", relPath);
						totalEdges++;
					}
				}
			}

			// Resolve imports: link importing file to imported symbols
			for (const { relPath, imports } of pendingImports) {
				for (const imp of imports) {
					const resolvedFile = this.resolveImportPath(imp.source, relPath);

					for (const spec of imp.specifiers) {
						const cleanSpec = spec.replace(/^\* as /, "");

						// Find the imported symbol in the target file
						let targetRow: { id: number } | undefined;
						if (resolvedFile) {
							targetRow = findSymbolInFile.get(cleanSpec, resolvedFile) as
								| { id: number }
								| undefined;
						}
						if (!targetRow) {
							targetRow = findSymbol.get(cleanSpec) as { id: number } | undefined;
						}
						if (!targetRow) continue;

						const sourceRow = findFirstSymbolInFile.get(relPath) as { id: number } | undefined;
						if (sourceRow) {
							insertEdge.run(sourceRow.id, targetRow.id, "imports", relPath);
							totalEdges++;
						}
					}
				}
			}
		});

		resolveEdges();
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

	private resolveImportPath(importSource: string, fromFile: string): string | null {
		if (!importSource.startsWith(".")) return null;

		const dir = dirname(fromFile);
		const base = join(dir, importSource).replace(/\\/g, "/");

		const extensions = ["", ".ts", ".tsx", ".js", ".jsx", ".mjs", "/index.ts", "/index.js"];
		for (const ext of extensions) {
			const candidate = base + ext;
			const row = this.db.prepare("SELECT path FROM files WHERE path = ?").get(candidate) as
				| { path: string }
				| undefined;
			if (row) return row.path;
		}
		return null;
	}

	private async getParser(): Promise<NativeParser> {
		if (this.nativeParser) return this.nativeParser;

		try {
			const native = await import("../../native/index.js");
			const fn = native.parseFile;
			if (native.available && fn) {
				this.nativeParser = {
					parseFile: (path, source, language) =>
						fn(path, source, language) as unknown as ParsedFile,
				};
			} else {
				this.nativeParser = new FallbackParser();
			}
		} catch {
			this.nativeParser = new FallbackParser();
		}
		return this.nativeParser!;
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
			callSites: [],
			imports: [],
		};
	}
}

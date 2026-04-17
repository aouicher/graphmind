import { existsSync, mkdirSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { FileHashCache } from "../src/core/graph/cache.js";
import { GraphQueries } from "../src/core/graph/queries.js";
import { initDatabase } from "../src/core/graph/schema.js";

const TEST_DIR = join(tmpdir(), `graphmind-test-graph-${Date.now()}`);

describe("Graph Schema", () => {
	let dbPath: string;

	beforeEach(() => {
		mkdirSync(TEST_DIR, { recursive: true });
		dbPath = join(TEST_DIR, "test.db");
	});

	afterEach(() => {
		rmSync(TEST_DIR, { recursive: true, force: true });
	});

	it("creates database with all tables", () => {
		const db = initDatabase(dbPath);
		const tables = db
			.prepare("SELECT name FROM sqlite_master WHERE type='table' ORDER BY name")
			.all() as Array<{ name: string }>;
		const names = tables.map((t) => t.name);

		expect(names).toContain("symbols");
		expect(names).toContain("edges");
		expect(names).toContain("files");
		expect(names).toContain("symbols_fts");
		db.close();
	});

	it("is idempotent (can init twice)", () => {
		const db1 = initDatabase(dbPath);
		db1.close();
		const db2 = initDatabase(dbPath);
		const tables = db2.prepare("SELECT name FROM sqlite_master WHERE type='table'").all();
		expect(tables.length).toBeGreaterThan(0);
		db2.close();
	});
});

describe("GraphQueries", () => {
	let dbPath: string;
	let queries: GraphQueries;
	let db: ReturnType<typeof initDatabase>;

	beforeEach(() => {
		mkdirSync(TEST_DIR, { recursive: true });
		dbPath = join(TEST_DIR, "queries.db");
		db = initDatabase(dbPath);
		queries = new GraphQueries(db);

		db.prepare(
			"INSERT INTO symbols (name, kind, file, line_start, line_end, signature) VALUES (?, ?, ?, ?, ?, ?)",
		).run(
			"authenticate",
			"function",
			"src/auth.ts",
			10,
			20,
			"(token: string): Promise<AuthResult>",
		);
		db.prepare(
			"INSERT INTO symbols (name, kind, file, line_start, line_end, signature) VALUES (?, ?, ?, ?, ?, ?)",
		).run(
			"validateToken",
			"function",
			"src/utils/jwt.ts",
			5,
			15,
			"(token: string): DecodedToken | null",
		);
		db.prepare(
			"INSERT INTO symbols (name, kind, file, line_start, line_end) VALUES (?, ?, ?, ?, ?)",
		).run("UserRepository", "class", "src/user.ts", 1, 30);
		db.prepare(
			"INSERT INTO symbols (name, kind, file, line_start, line_end) VALUES (?, ?, ?, ?, ?)",
		).run("handleRequest", "function", "src/index.ts", 5, 10);

		db.prepare("INSERT INTO edges (from_id, to_id, kind, file) VALUES (?, ?, ?, ?)").run(
			1,
			2,
			"calls",
			"src/auth.ts",
		);
		db.prepare("INSERT INTO edges (from_id, to_id, kind, file) VALUES (?, ?, ?, ?)").run(
			4,
			1,
			"calls",
			"src/index.ts",
		);
	});

	afterEach(() => {
		db.close();
		rmSync(TEST_DIR, { recursive: true, force: true });
	});

	it("finds symbols by name", () => {
		const results = queries.findSymbol("authenticate");
		expect(results).toHaveLength(1);
		expect(results[0]?.kind).toBe("function");
		expect(results[0]?.file).toBe("src/auth.ts");
	});

	it("finds callers", () => {
		const callers = queries.callers("authenticate");
		expect(callers).toHaveLength(1);
		expect(callers[0]?.name).toBe("handleRequest");
	});

	it("finds callees", () => {
		const callees = queries.callees("authenticate");
		expect(callees).toHaveLength(1);
		expect(callees[0]?.name).toBe("validateToken");
	});

	it("searches via FTS", () => {
		const results = queries.searchSymbols("auth*");
		expect(results.length).toBeGreaterThan(0);
		expect(results[0]?.name).toBe("authenticate");
	});

	it("computes file dependencies", () => {
		const deps = queries.fileDeps("src/auth.ts");
		expect(deps).toHaveLength(1);
		expect(deps[0]?.file).toBe("src/utils/jwt.ts");
	});

	it("computes reverse file dependencies", () => {
		const rdeps = queries.fileReverseDeps("src/auth.ts");
		expect(rdeps).toHaveLength(1);
		expect(rdeps[0]?.file).toBe("src/index.ts");
	});

	it("computes impact (transitive reverse deps)", () => {
		const impacted = queries.impact("src/utils/jwt.ts");
		expect(impacted).toContain("src/auth.ts");
		expect(impacted).toContain("src/index.ts");
	});

	it("returns stats", () => {
		const stats = queries.stats();
		expect(stats.symbols).toBe(4);
		expect(stats.edges).toBe(2);
	});

	it("detects top connected files", () => {
		const top = queries.topConnected(5);
		expect(top.length).toBeGreaterThan(0);
	});

	it("detects cycles", () => {
		const cycles = queries.detectCycles();
		expect(cycles).toHaveLength(0);
	});
});

describe("FileHashCache", () => {
	const cacheDir = join(TEST_DIR, "cache");

	beforeEach(() => {
		mkdirSync(cacheDir, { recursive: true });
	});

	afterEach(() => {
		rmSync(TEST_DIR, { recursive: true, force: true });
	});

	it("detects file changes", () => {
		const cache = new FileHashCache(cacheDir);
		expect(cache.hasChanged("file.ts", "content v1")).toBe(true);
		cache.update("file.ts", "content v1");
		expect(cache.hasChanged("file.ts", "content v1")).toBe(false);
		expect(cache.hasChanged("file.ts", "content v2")).toBe(true);
	});

	it("persists across instances", () => {
		const cache1 = new FileHashCache(cacheDir);
		cache1.update("file.ts", "hello");
		cache1.save();

		const cache2 = new FileHashCache(cacheDir);
		expect(cache2.hasChanged("file.ts", "hello")).toBe(false);
	});
});

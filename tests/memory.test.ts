import { existsSync, mkdirSync, rmSync } from "node:fs";
import { join } from "node:path";
import { tmpdir } from "node:os";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { MemoryStore } from "../src/core/memory/store.js";
import { MemorySearch } from "../src/core/memory/search.js";
import { MemoryIndex } from "../src/core/memory/index.js";

const TEST_DIR = join(tmpdir(), "graphmind-test-memory-" + Date.now());

describe("MemoryStore", () => {
	beforeEach(() => {
		mkdirSync(TEST_DIR, { recursive: true });
		vi.stubEnv("HOME", TEST_DIR);
	});

	afterEach(() => {
		rmSync(TEST_DIR, { recursive: true, force: true });
		vi.unstubAllEnvs();
	});

	it("adds and lists global memory entries", () => {
		const store = new MemoryStore();
		store.add("JWT tokens expire after 1 hour", { global: true, type: "decision", tags: ["auth", "jwt"] });
		store.add("Use snake_case for DB columns", { global: true, type: "convention" });

		const entries = store.list();
		expect(entries).toHaveLength(2);
		expect(entries[0]!.content).toContain("snake_case");
		expect(entries[1]!.content).toContain("JWT");
	});

	it("adds project-scoped entries", () => {
		const store = new MemoryStore();
		store.add("Wallet uses PostgreSQL", { project: "wallet-service", type: "context" });
		store.add("Global fact", { global: true });

		const all = store.list("wallet-service");
		expect(all).toHaveLength(2);

		const globalOnly = store.list();
		expect(globalOnly).toHaveLength(1);
	});

	it("deletes entries by id", () => {
		const store = new MemoryStore();
		const entry = store.add("To be deleted", { global: true });

		expect(store.list()).toHaveLength(1);
		const deleted = store.delete(entry.id);
		expect(deleted).toBe(true);
		expect(store.list()).toHaveLength(0);
	});

	it("returns false when deleting non-existent entry", () => {
		const store = new MemoryStore();
		expect(store.delete("non-existent-id")).toBe(false);
	});
});

describe("MemorySearch", () => {
	it("finds entries by keyword", () => {
		const search = new MemorySearch();
		const entries = [
			{ id: "1", created: "2024-01-01", updated: "2024-01-01", project: null, global: true, type: "decision" as const, content: "Use JWT for authentication", tags: ["auth"], session: "2024-01-01" },
			{ id: "2", created: "2024-01-02", updated: "2024-01-02", project: null, global: true, type: "convention" as const, content: "PostgreSQL for all services", tags: ["db"], session: "2024-01-02" },
			{ id: "3", created: "2024-01-03", updated: "2024-01-03", project: null, global: true, type: "pattern" as const, content: "Auth middleware validates JWT tokens", tags: ["auth", "jwt"], session: "2024-01-03" },
		];

		const results = search.search(entries, "JWT auth");
		expect(results.length).toBeGreaterThan(0);
		expect(results[0]!.content).toContain("JWT");
	});

	it("ranks tag matches higher", () => {
		const search = new MemorySearch();
		const entries = [
			{ id: "1", created: "2024-01-01", updated: "2024-01-01", project: null, global: true, type: "context" as const, content: "Some text about auth", tags: [], session: "2024-01-01" },
			{ id: "2", created: "2024-01-02", updated: "2024-01-02", project: null, global: true, type: "context" as const, content: "Another entry", tags: ["auth"], session: "2024-01-02" },
		];

		const results = search.search(entries, "auth");
		expect(results).toHaveLength(2);
		expect(results[0]!.id).toBe("2");
	});

	it("returns empty for no matches", () => {
		const search = new MemorySearch();
		const results = search.search([], "anything");
		expect(results).toHaveLength(0);
	});
});

describe("MemoryIndex", () => {
	it("indexes entries by tag and project", () => {
		const index = new MemoryIndex();
		const entries = [
			{ id: "1", created: "2024-01-01", updated: "2024-01-01", project: "api", global: false, type: "decision" as const, content: "Use REST", tags: ["api", "rest"], session: "2024-01-01" },
			{ id: "2", created: "2024-01-02", updated: "2024-01-02", project: "api", global: false, type: "pattern" as const, content: "Middleware pattern", tags: ["api"], session: "2024-01-02" },
		];

		index.build(entries);

		expect(index.findByTag("api")).toHaveLength(2);
		expect(index.findByTag("rest")).toHaveLength(1);
		expect(index.findByProject("api")).toHaveLength(2);
		expect(index.get("1")!.content).toBe("Use REST");
	});
});

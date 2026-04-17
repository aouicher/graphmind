import { mkdirSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { GraphQueries } from "../src/core/graph/queries.js";
import { initDatabase } from "../src/core/graph/schema.js";

const TEST_DIR = join(tmpdir(), `graphmind-test-diffimpact-${Date.now()}`);

describe("diff-impact logic", () => {
	let db: ReturnType<typeof initDatabase>;
	let queries: GraphQueries;

	beforeEach(() => {
		mkdirSync(TEST_DIR, { recursive: true });
		const dbPath = join(TEST_DIR, "impact.db");
		db = initDatabase(dbPath);
		queries = new GraphQueries(db);

		db.prepare(
			"INSERT INTO symbols (name, kind, file, line_start, line_end) VALUES (?, ?, ?, ?, ?)",
		).run("handler", "function", "src/api/handler.ts", 1, 20);
		db.prepare(
			"INSERT INTO symbols (name, kind, file, line_start, line_end) VALUES (?, ?, ?, ?, ?)",
		).run("validate", "function", "src/core/validate.ts", 1, 15);
		db.prepare(
			"INSERT INTO symbols (name, kind, file, line_start, line_end) VALUES (?, ?, ?, ?, ?)",
		).run("utils", "function", "src/utils/helpers.ts", 1, 10);
		db.prepare(
			"INSERT INTO symbols (name, kind, file, line_start, line_end) VALUES (?, ?, ?, ?, ?)",
		).run("entry", "function", "src/index.ts", 1, 5);

		// handler calls validate, validate calls utils, entry calls handler
		db.prepare("INSERT INTO edges (from_id, to_id, kind, file) VALUES (?, ?, ?, ?)").run(
			1,
			2,
			"calls",
			"src/api/handler.ts",
		);
		db.prepare("INSERT INTO edges (from_id, to_id, kind, file) VALUES (?, ?, ?, ?)").run(
			2,
			3,
			"calls",
			"src/core/validate.ts",
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

	it("traces impact of leaf file change", () => {
		const impacted = queries.impact("src/utils/helpers.ts");
		expect(impacted).toContain("src/core/validate.ts");
		expect(impacted).toContain("src/api/handler.ts");
		expect(impacted).toContain("src/index.ts");
	});

	it("traces impact of mid-level file change", () => {
		const impacted = queries.impact("src/core/validate.ts");
		expect(impacted).toContain("src/api/handler.ts");
		expect(impacted).toContain("src/index.ts");
		expect(impacted).not.toContain("src/utils/helpers.ts");
	});

	it("traces impact of root file change", () => {
		const impacted = queries.impact("src/index.ts");
		expect(impacted).toHaveLength(0);
	});

	it("respects depth limit", () => {
		const impacted = queries.impact("src/utils/helpers.ts", 1);
		expect(impacted).toContain("src/core/validate.ts");
		expect(impacted).not.toContain("src/index.ts");
	});

	it("handles multiple changed files", () => {
		const allImpacted = new Set<string>();
		const changedFiles = ["src/utils/helpers.ts", "src/api/handler.ts"];

		for (const file of changedFiles) {
			for (const f of queries.impact(file)) {
				allImpacted.add(f);
			}
		}

		const nonChanged = [...allImpacted].filter((f) => !changedFiles.includes(f));
		expect(nonChanged).toContain("src/core/validate.ts");
		expect(nonChanged).toContain("src/index.ts");
	});

	it("returns empty for unknown file", () => {
		const impacted = queries.impact("src/nonexistent.ts");
		expect(impacted).toHaveLength(0);
	});
});

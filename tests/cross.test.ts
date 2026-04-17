import { mkdirSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { CrossLinkStore } from "../src/core/cross/links.js";

const TEST_DIR = join(tmpdir(), `graphmind-test-cross-${Date.now()}`);

describe("CrossLinkStore", () => {
	beforeEach(() => {
		mkdirSync(TEST_DIR, { recursive: true });
		vi.stubEnv("HOME", TEST_DIR);
	});

	afterEach(() => {
		rmSync(TEST_DIR, { recursive: true, force: true });
		vi.unstubAllEnvs();
	});

	it("adds and lists cross-project links", () => {
		const store = new CrossLinkStore();
		store.add({
			from: "api",
			to: "shared-lib",
			type: "depends-on",
			reason: "API imports shared types",
			symbols: ["AuthToken", "UserDTO"],
			inferred: false,
			confidence: 1.0,
		});

		const links = store.list();
		expect(links).toHaveLength(1);
		expect(links[0]?.from).toBe("api");
		expect(links[0]?.to).toBe("shared-lib");
		expect(links[0]?.type).toBe("depends-on");
		expect(links[0]?.symbols).toEqual(["AuthToken", "UserDTO"]);
		expect(links[0]?.id).toBeTruthy();
		expect(links[0]?.created).toBeTruthy();
	});

	it("finds links by project slug", () => {
		const store = new CrossLinkStore();
		store.add({
			from: "api",
			to: "shared",
			type: "depends-on",
			reason: "r1",
			symbols: [],
			inferred: false,
			confidence: 1.0,
		});
		store.add({
			from: "web",
			to: "shared",
			type: "uses-types-from",
			reason: "r2",
			symbols: [],
			inferred: false,
			confidence: 1.0,
		});
		store.add({
			from: "api",
			to: "web",
			type: "shares-pattern",
			reason: "r3",
			symbols: [],
			inferred: true,
			confidence: 0.7,
		});

		const sharedLinks = store.findByProject("shared");
		expect(sharedLinks).toHaveLength(2);

		const apiLinks = store.findByProject("api");
		expect(apiLinks).toHaveLength(2);

		const webLinks = store.findByProject("web");
		expect(webLinks).toHaveLength(2);
	});

	it("deletes links by id", () => {
		const store = new CrossLinkStore();
		const link = store.add({
			from: "a",
			to: "b",
			type: "depends-on",
			reason: "test",
			symbols: [],
			inferred: false,
			confidence: 1.0,
		});

		expect(store.list()).toHaveLength(1);
		const deleted = store.delete(link.id);
		expect(deleted).toBe(true);
		expect(store.list()).toHaveLength(0);
	});

	it("returns false when deleting non-existent link", () => {
		const store = new CrossLinkStore();
		expect(store.delete("non-existent")).toBe(false);
	});

	it("returns empty list when no links file exists", () => {
		const store = new CrossLinkStore();
		expect(store.list()).toEqual([]);
	});

	it("preserves inferred flag and confidence", () => {
		const store = new CrossLinkStore();
		store.add({
			from: "a",
			to: "b",
			type: "shares-pattern",
			reason: "shared sym",
			symbols: ["Foo"],
			inferred: true,
			confidence: 0.7,
		});

		const links = store.list();
		expect(links[0]?.inferred).toBe(true);
		expect(links[0]?.confidence).toBe(0.7);
	});
});

import { existsSync, mkdirSync, readFileSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { SessionLogger } from "../src/core/session/logger.js";

const TEST_DIR = join(tmpdir(), `graphmind-test-session-${Date.now()}`);

describe("SessionLogger", () => {
	beforeEach(() => {
		mkdirSync(TEST_DIR, { recursive: true });
		vi.stubEnv("HOME", TEST_DIR);
	});

	afterEach(() => {
		rmSync(TEST_DIR, { recursive: true, force: true });
		vi.unstubAllEnvs();
	});

	it("creates session log on start", () => {
		const logger = new SessionLogger();
		const logPath = logger.start("my-project");

		expect(existsSync(logPath)).toBe(true);
		const content = readFileSync(logPath, "utf-8");
		expect(content).toContain("my-project");
		expect(content).toContain("Session started.");
	});

	it("saves session message", () => {
		const logger = new SessionLogger();
		const logPath = logger.save("my-project", "Refactored auth module");

		const content = readFileSync(logPath, "utf-8");
		expect(content).toContain("my-project");
		expect(content).toContain("Refactored auth module");
	});

	it("appends multiple entries to same day log", () => {
		const logger = new SessionLogger();
		logger.start("project-a");
		logger.save("project-a", "First task done");
		logger.save("project-b", "Another project");

		const date = new Date().toISOString().slice(0, 10);
		const sessionsPath = join(TEST_DIR, ".graphmind", "sessions", `${date}.md`);
		const content = readFileSync(sessionsPath, "utf-8");

		expect(content).toContain("project-a");
		expect(content).toContain("project-b");
		expect(content).toContain("First task done");
		expect(content).toContain("Another project");
	});

	it("returns history entries", () => {
		const logger = new SessionLogger();
		logger.start("proj-a");
		logger.save("proj-a", "Did something");
		logger.save("proj-b", "Did something else");

		const all = logger.history();
		expect(all.length).toBeGreaterThanOrEqual(3);
	});

	it("filters history by slug", () => {
		const logger = new SessionLogger();
		logger.start("proj-a");
		logger.save("proj-b", "B work");

		const aHistory = logger.history("proj-a");
		for (const entry of aHistory) {
			expect(entry).toContain("proj-a");
		}

		const bHistory = logger.history("proj-b");
		for (const entry of bHistory) {
			expect(entry).toContain("proj-b");
		}
	});

	it("respects limit parameter", () => {
		const logger = new SessionLogger();
		for (let i = 0; i < 5; i++) {
			logger.save("proj", `Entry ${i}`);
		}

		const limited = logger.history(undefined, 3);
		expect(limited).toHaveLength(3);
	});

	it("returns empty history when no sessions exist", () => {
		const logger = new SessionLogger();
		const history = logger.history();
		expect(history).toEqual([]);
	});
});

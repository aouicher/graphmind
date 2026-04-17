import { createHash } from "node:crypto";
import { existsSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { join } from "node:path";

export class FileHashCache {
	private cache: Map<string, string>;
	private cacheDir: string;
	private cacheFile: string;

	constructor(cacheDir: string) {
		this.cacheDir = cacheDir;
		this.cacheFile = join(cacheDir, "hashes.json");
		this.cache = new Map();
		this.load();
	}

	hasChanged(filePath: string, content: string): boolean {
		const hash = this.hash(content);
		const existing = this.cache.get(filePath);
		return existing !== hash;
	}

	update(filePath: string, content: string): void {
		this.cache.set(filePath, this.hash(content));
	}

	remove(filePath: string): void {
		this.cache.delete(filePath);
	}

	save(): void {
		mkdirSync(this.cacheDir, { recursive: true });
		const obj = Object.fromEntries(this.cache);
		writeFileSync(this.cacheFile, JSON.stringify(obj));
	}

	private load(): void {
		if (!existsSync(this.cacheFile)) return;
		const raw = readFileSync(this.cacheFile, "utf-8");
		const obj = JSON.parse(raw) as Record<string, string>;
		this.cache = new Map(Object.entries(obj));
	}

	private hash(content: string): string {
		return createHash("sha256").update(content).digest("hex");
	}
}

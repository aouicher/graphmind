import { homedir } from "node:os";
import { join, resolve } from "node:path";

function home(): string {
	return process.env.HOME ?? homedir();
}

export function graphmindDir(): string {
	return join(home(), ".graphmind");
}

export function configPath(): string {
	return join(graphmindDir(), "config.json");
}

export function memoryDir(): string {
	return join(graphmindDir(), "memory");
}

export function graphsDir(): string {
	return join(graphmindDir(), "graphs");
}

export function crossLinksDir(): string {
	return join(graphmindDir(), "cross-links");
}

export function sessionsDir(): string {
	return join(graphmindDir(), "sessions");
}

export function graphDir(slug: string): string {
	return join(graphsDir(), slug);
}

export function graphDbPath(slug: string): string {
	return join(graphDir(slug), "graph.db");
}

export function memoryPath(slug: string): string {
	return join(memoryDir(), `${slug}.jsonl`);
}

export function globalMemoryPath(): string {
	return join(memoryDir(), "global.jsonl");
}

export function crossLinksPath(): string {
	return join(crossLinksDir(), "links.jsonl");
}

export function sessionLogPath(date: string): string {
	return join(sessionsDir(), `${date}.md`);
}

export function metaPath(slug: string): string {
	return join(graphDir(slug), "meta.json");
}

export function cacheDirPath(slug: string): string {
	return join(graphDir(slug), "cache");
}

export function safePath(path: string, allowedRoots: string[]): string {
	const resolved = resolve(path);
	const isAllowed = allowedRoots.some((root) => resolved.startsWith(resolve(root)));
	if (!isAllowed) {
		throw new Error(`Path traversal blocked: ${path} is outside allowed boundaries`);
	}
	return resolved;
}

import {
	appendFileSync,
	existsSync,
	mkdirSync,
	readFileSync,
	renameSync,
	writeFileSync,
} from "node:fs";
import { dirname } from "node:path";
import { v4 as uuid } from "uuid";
import { memoryDir, globalMemoryPath, memoryPath } from "../../utils/paths.js";

export interface MemoryEntry {
	id: string;
	created: string;
	updated: string;
	project: string | null;
	global: boolean;
	type: "decision" | "pattern" | "convention" | "bug" | "context" | "session";
	content: string;
	tags: string[];
	session: string;
}

export class MemoryStore {
	constructor() {
		mkdirSync(memoryDir(), { recursive: true });
	}

	add(
		content: string,
		options?: {
			project?: string;
			global?: boolean;
			type?: MemoryEntry["type"];
			tags?: string[];
		},
	): MemoryEntry {
		const now = new Date().toISOString();
		const entry: MemoryEntry = {
			id: uuid(),
			created: now,
			updated: now,
			project: options?.project ?? null,
			global: options?.global ?? false,
			type: options?.type ?? "context",
			content,
			tags: options?.tags ?? [],
			session: now.slice(0, 10),
		};

		const filePath = entry.global
			? globalMemoryPath()
			: entry.project
				? memoryPath(entry.project)
				: globalMemoryPath();

		this.atomicAppend(filePath, JSON.stringify(entry));
		return entry;
	}

	list(project?: string): MemoryEntry[] {
		const entries: MemoryEntry[] = [];

		const globalPath = globalMemoryPath();
		if (existsSync(globalPath)) {
			entries.push(...this.readJsonl(globalPath));
		}

		if (project) {
			const projPath = memoryPath(project);
			if (existsSync(projPath)) {
				entries.push(...this.readJsonl(projPath));
			}
		}

		return entries.sort(
			(a, b) => new Date(b.created).getTime() - new Date(a.created).getTime(),
		);
	}

	delete(id: string, project?: string): boolean {
		const paths = [globalMemoryPath()];
		if (project) paths.push(memoryPath(project));

		for (const filePath of paths) {
			if (!existsSync(filePath)) continue;
			const entries = this.readJsonl(filePath);
			const filtered = entries.filter((e) => e.id !== id);
			if (filtered.length !== entries.length) {
				this.atomicWrite(
					filePath,
					filtered.map((e) => JSON.stringify(e)).join("\n") + (filtered.length ? "\n" : ""),
				);
				return true;
			}
		}
		return false;
	}

	private readJsonl(filePath: string): MemoryEntry[] {
		const content = readFileSync(filePath, "utf-8");
		return content
			.split("\n")
			.filter((line) => line.trim())
			.map((line) => JSON.parse(line) as MemoryEntry);
	}

	private atomicAppend(filePath: string, line: string): void {
		mkdirSync(dirname(filePath), { recursive: true });
		appendFileSync(filePath, `${line}\n`);
	}

	private atomicWrite(filePath: string, content: string): void {
		const tmpPath = `${filePath}.tmp.${Date.now()}`;
		writeFileSync(tmpPath, content);
		renameSync(tmpPath, filePath);
	}
}

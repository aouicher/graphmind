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
import { crossLinksPath } from "../../utils/paths.js";

export interface CrossLink {
	id: string;
	from: string;
	to: string;
	type: "depends-on" | "shares-pattern" | "extends" | "uses-types-from";
	reason: string;
	symbols: string[];
	inferred: boolean;
	confidence: number;
	created: string;
}

export class CrossLinkStore {
	add(link: Omit<CrossLink, "id" | "created">): CrossLink {
		const entry: CrossLink = {
			...link,
			id: uuid(),
			created: new Date().toISOString(),
		};

		const filePath = crossLinksPath();
		mkdirSync(dirname(filePath), { recursive: true });
		appendFileSync(filePath, `${JSON.stringify(entry)}\n`);
		return entry;
	}

	list(): CrossLink[] {
		const filePath = crossLinksPath();
		if (!existsSync(filePath)) return [];
		return readFileSync(filePath, "utf-8")
			.split("\n")
			.filter((l) => l.trim())
			.map((l) => JSON.parse(l) as CrossLink);
	}

	findByProject(slug: string): CrossLink[] {
		return this.list().filter((l) => l.from === slug || l.to === slug);
	}

	delete(id: string): boolean {
		const filePath = crossLinksPath();
		if (!existsSync(filePath)) return false;
		const entries = this.list();
		const filtered = entries.filter((e) => e.id !== id);
		if (filtered.length === entries.length) return false;
		const tmpPath = `${filePath}.tmp.${Date.now()}`;
		writeFileSync(
			tmpPath,
			filtered.map((e) => JSON.stringify(e)).join("\n") + (filtered.length ? "\n" : ""),
		);
		renameSync(tmpPath, filePath);
		return true;
	}
}

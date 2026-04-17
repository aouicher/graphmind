import { existsSync } from "node:fs";
import { graphDbPath } from "../../utils/paths.js";
import { initDatabase } from "../graph/schema.js";
import { Registry } from "../registry.js";
import { type CrossLink, CrossLinkStore } from "./links.js";

interface SharedSymbol {
	name: string;
	kind: string;
	projects: string[];
}

export function inferCrossLinks(): CrossLink[] {
	const registry = new Registry();
	const projects = registry.list();
	const store = new CrossLinkStore();
	const existing = store.list();

	const symbolMap = new Map<string, Set<string>>();

	for (const project of projects) {
		const dbPath = graphDbPath(project.slug);
		if (!existsSync(dbPath)) continue;

		const db = initDatabase(dbPath);
		const symbols = db.prepare("SELECT DISTINCT name, kind FROM symbols").all() as Array<{
			name: string;
			kind: string;
		}>;
		db.close();

		for (const sym of symbols) {
			const key = `${sym.name}:${sym.kind}`;
			if (!symbolMap.has(key)) {
				symbolMap.set(key, new Set());
			}
			symbolMap.get(key)?.add(project.slug);
		}
	}

	const shared: SharedSymbol[] = [];
	for (const [key, projectSet] of symbolMap) {
		if (projectSet.size > 1) {
			const [name, kind] = key.split(":");
			shared.push({ name: name!, kind: kind!, projects: [...projectSet] });
		}
	}

	const newLinks: CrossLink[] = [];
	const linkPairs = new Set(existing.map((l) => `${l.from}:${l.to}`));

	for (const sym of shared) {
		for (let i = 0; i < sym.projects.length; i++) {
			for (let j = i + 1; j < sym.projects.length; j++) {
				const from = sym.projects[i]!;
				const to = sym.projects[j]!;
				const pairKey = `${from}:${to}`;
				const reversePairKey = `${to}:${from}`;

				if (linkPairs.has(pairKey) || linkPairs.has(reversePairKey)) continue;

				const link = store.add({
					from,
					to,
					type: "shares-pattern",
					reason: `Shared symbol: ${sym.name} (${sym.kind})`,
					symbols: [sym.name],
					inferred: true,
					confidence: 0.7,
				});
				newLinks.push(link);
				linkPairs.add(pairKey);
			}
		}
	}

	return newLinks;
}

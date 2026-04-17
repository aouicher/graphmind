import type { MemoryEntry } from "./store.js";

export class MemorySearch {
	search(entries: MemoryEntry[], query: string, limit = 20): MemoryEntry[] {
		const terms = query
			.toLowerCase()
			.split(/\s+/)
			.filter((t) => t.length > 1);

		if (terms.length === 0) return entries.slice(0, limit);

		const scored = entries.map((entry) => {
			let score = 0;
			const content = entry.content.toLowerCase();
			const tags = entry.tags.map((t) => t.toLowerCase());

			for (const term of terms) {
				if (content.includes(term)) {
					score += 1;
					const regex = new RegExp(term, "gi");
					const matches = content.match(regex);
					if (matches) score += matches.length * 0.1;
				}

				if (tags.includes(term)) {
					score += 2;
				}

				if (entry.type === term) {
					score += 1.5;
				}
			}

			return { entry, score };
		});

		return scored
			.filter((s) => s.score > 0)
			.sort((a, b) => b.score - a.score)
			.slice(0, limit)
			.map((s) => s.entry);
	}
}

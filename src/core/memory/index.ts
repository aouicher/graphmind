import type { MemoryEntry } from "./store.js";

export class MemoryIndex {
	private byTag: Map<string, Set<string>> = new Map();
	private byProject: Map<string, Set<string>> = new Map();
	private entries: Map<string, MemoryEntry> = new Map();

	build(entries: MemoryEntry[]): void {
		this.byTag.clear();
		this.byProject.clear();
		this.entries.clear();

		for (const entry of entries) {
			this.entries.set(entry.id, entry);

			for (const tag of entry.tags) {
				const tagLower = tag.toLowerCase();
				if (!this.byTag.has(tagLower)) {
					this.byTag.set(tagLower, new Set());
				}
				this.byTag.get(tagLower)?.add(entry.id);
			}

			const proj = entry.project ?? "__global__";
			if (!this.byProject.has(proj)) {
				this.byProject.set(proj, new Set());
			}
			this.byProject.get(proj)?.add(entry.id);
		}
	}

	findByTag(tag: string): MemoryEntry[] {
		const ids = this.byTag.get(tag.toLowerCase());
		if (!ids) return [];
		return [...ids].map((id) => this.entries.get(id)).filter(Boolean) as MemoryEntry[];
	}

	findByProject(project: string): MemoryEntry[] {
		const ids = this.byProject.get(project);
		if (!ids) return [];
		return [...ids].map((id) => this.entries.get(id)).filter(Boolean) as MemoryEntry[];
	}

	get(id: string): MemoryEntry | undefined {
		return this.entries.get(id);
	}
}

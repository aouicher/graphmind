import { Registry } from "../core/registry.js";

export function resolveProjectSlug(
	...candidates: Array<string | undefined | null>
): string | undefined {
	const registry = new Registry();
	for (const c of candidates) {
		if (c) return c;
	}
	const byPath = registry.findByPath(process.cwd());
	if (byPath) return byPath.slug;
	const all = registry.list();
	if (all.length === 1) return all[0]?.slug;
	return undefined;
}

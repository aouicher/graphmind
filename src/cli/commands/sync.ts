import type { Command } from "commander";
import { existsSync, readFileSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { Registry } from "../../core/registry.js";
import { log } from "../../utils/logger.js";
import { metaPath } from "../../utils/paths.js";
import { graphDbPath } from "../../utils/paths.js";
import { initDatabase } from "../../core/graph/schema.js";
import { GraphQueries } from "../../core/graph/queries.js";

const SECTION_START = "<!-- graphmind:start -->";
const SECTION_END = "<!-- graphmind:end -->";

export function registerSyncCommand(program: Command): void {
	program
		.command("sync [slug]")
		.description("Update CLAUDE.md with graph context")
		.option("--all", "Sync all projects")
		.action((slug: string | undefined, opts: { all?: boolean }) => {
			const registry = new Registry();
			const projects = opts.all
				? registry.list()
				: slug
					? [registry.get(slug)].filter(Boolean)
					: [registry.findByPath(process.cwd())].filter(Boolean);

			if (projects.length === 0) {
				log.error("No project found. Run: graphmind register");
				process.exitCode = 1;
				return;
			}

			for (const project of projects) {
				if (!project) continue;
				const claudeMd = join(project.path, "CLAUDE.md");
				const section = buildSection(project.slug);

				if (!section) {
					log.warn(`No graph data for "${project.slug}". Build first.`);
					continue;
				}

				let content = "";
				if (existsSync(claudeMd)) {
					content = readFileSync(claudeMd, "utf-8");
				}

				const wrappedSection = `${SECTION_START}\n${section}\n${SECTION_END}`;

				if (content.includes(SECTION_START)) {
					const regex = new RegExp(
						`${escapeRegex(SECTION_START)}[\\s\\S]*?${escapeRegex(SECTION_END)}`,
					);
					content = content.replace(regex, wrappedSection);
				} else {
					content = content ? `${content}\n\n${wrappedSection}\n` : `${wrappedSection}\n`;
				}

				writeFileSync(claudeMd, content);
				log.success(`Synced CLAUDE.md for "${project.slug}"`);
			}
		});
}

function buildSection(slug: string): string | null {
	const dbPath = graphDbPath(slug);
	if (!existsSync(dbPath)) return null;

	const db = initDatabase(dbPath);
	const q = new GraphQueries(db);
	const stats = q.stats();
	const langs = q.languageBreakdown();
	db.close();

	let meta: { lastBuild?: string } = {};
	const mp = metaPath(slug);
	if (existsSync(mp)) {
		meta = JSON.parse(readFileSync(mp, "utf-8"));
	}

	const langStr = langs.map((l) => `${l.language} (${l.count})`).join(", ");

	return `## graphmind

Last build: ${meta.lastBuild?.slice(0, 10) ?? "never"} | ${stats.symbols} symbols | ${stats.edges} edges | ${stats.files} files
Languages: ${langStr || "none"}
MCP: \`graphmind mcp\` (stdio)

### Before editing anything
- Symbol: \`graphmind fn <symbol> --no-tests\`
- File: \`graphmind deps <file>\`
- Git changes: \`graphmind diff-impact\`
- Find by intent: \`graphmind search "handle auth; validate token"\`

### Rebuild when
Structural changes, new modules, after merge.
Command: \`graphmind build --incremental\``;
}

function escapeRegex(str: string): string {
	return str.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
}

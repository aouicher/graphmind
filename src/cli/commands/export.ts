import { existsSync } from "node:fs";
import type { Command } from "commander";
import { CrossLinkStore } from "../../core/cross/links.js";
import { initDatabase } from "../../core/graph/schema.js";
import { Registry } from "../../core/registry.js";
import { log } from "../../utils/logger.js";
import { graphDbPath } from "../../utils/paths.js";

type Format = "dot" | "mermaid" | "json";

interface GraphEdge {
	from_file: string;
	to_file: string;
	kind: string;
}

function getFileEdges(db: ReturnType<typeof initDatabase>): GraphEdge[] {
	return db
		.prepare(`
			SELECT DISTINCT s1.file as from_file, s2.file as to_file, e.kind
			FROM edges e
			JOIN symbols s1 ON e.from_id = s1.id
			JOIN symbols s2 ON e.to_id = s2.id
			WHERE s1.file != s2.file
		`)
		.all() as GraphEdge[];
}

function sanitizeId(s: string): string {
	return s.replace(/[^a-zA-Z0-9_]/g, "_");
}

function exportDot(edges: GraphEdge[], title: string): string {
	const lines = [`digraph "${title}" {`, "  rankdir=LR;", '  node [shape=box, style=filled, fillcolor="#e8e8e8"];'];
	const files = new Set<string>();
	for (const e of edges) {
		files.add(e.from_file);
		files.add(e.to_file);
	}
	for (const f of files) {
		lines.push(`  ${sanitizeId(f)} [label="${f}"];`);
	}
	for (const e of edges) {
		lines.push(`  ${sanitizeId(e.from_file)} -> ${sanitizeId(e.to_file)} [label="${e.kind}"];`);
	}
	lines.push("}");
	return lines.join("\n");
}

function exportMermaid(edges: GraphEdge[], title: string): string {
	const lines = [`graph LR`, `  %% ${title}`];
	const files = new Set<string>();
	for (const e of edges) {
		files.add(e.from_file);
		files.add(e.to_file);
	}
	for (const f of files) {
		lines.push(`  ${sanitizeId(f)}["${f}"]`);
	}
	for (const e of edges) {
		lines.push(`  ${sanitizeId(e.from_file)} -->|${e.kind}| ${sanitizeId(e.to_file)}`);
	}
	return lines.join("\n");
}

function exportJson(edges: GraphEdge[], title: string): string {
	const files = new Set<string>();
	for (const e of edges) {
		files.add(e.from_file);
		files.add(e.to_file);
	}
	return JSON.stringify({
		title,
		nodes: [...files].map((f) => ({ id: f })),
		edges: edges.map((e) => ({ from: e.from_file, to: e.to_file, kind: e.kind })),
	}, null, 2);
}

function exportCrossDot(): string {
	const store = new CrossLinkStore();
	const links = store.list();
	const lines = ['digraph "cross-project" {', "  rankdir=LR;", '  node [shape=box, style=filled, fillcolor="#d4e6f1"];'];
	const projects = new Set<string>();
	for (const l of links) {
		projects.add(l.from);
		projects.add(l.to);
	}
	for (const p of projects) {
		lines.push(`  ${sanitizeId(p)} [label="${p}"];`);
	}
	for (const l of links) {
		lines.push(`  ${sanitizeId(l.from)} -> ${sanitizeId(l.to)} [label="${l.type}"];`);
	}
	lines.push("}");
	return lines.join("\n");
}

function exportCrossMermaid(): string {
	const store = new CrossLinkStore();
	const links = store.list();
	const lines = ["graph LR", "  %% cross-project"];
	const projects = new Set<string>();
	for (const l of links) {
		projects.add(l.from);
		projects.add(l.to);
	}
	for (const p of projects) {
		lines.push(`  ${sanitizeId(p)}["${p}"]`);
	}
	for (const l of links) {
		lines.push(`  ${sanitizeId(l.from)} -->|${l.type}| ${sanitizeId(l.to)}`);
	}
	return lines.join("\n");
}

function exportCrossJson(): string {
	const store = new CrossLinkStore();
	const links = store.list();
	const projects = new Set<string>();
	for (const l of links) {
		projects.add(l.from);
		projects.add(l.to);
	}
	return JSON.stringify({
		title: "cross-project",
		nodes: [...projects].map((p) => ({ id: p })),
		edges: links.map((l) => ({ from: l.from, to: l.to, type: l.type, reason: l.reason })),
	}, null, 2);
}

export function registerExportCommand(program: Command): void {
	program
		.command("export [slug]")
		.description("Export graph as dot, mermaid, or json")
		.requiredOption("-f, --format <format>", "Output format: dot, mermaid, json")
		.option("--cross", "Export cross-project graph instead")
		.action((slug: string | undefined, opts: { format: string; cross?: boolean }) => {
			const format = opts.format as Format;
			if (!["dot", "mermaid", "json"].includes(format)) {
				log.error(`Unknown format: ${opts.format}. Use: dot, mermaid, json`);
				process.exitCode = 1;
				return;
			}

			if (opts.cross) {
				const output = format === "dot" ? exportCrossDot()
					: format === "mermaid" ? exportCrossMermaid()
					: exportCrossJson();
				console.log(output);
				return;
			}

			const registry = new Registry();
			const resolvedSlug = slug ?? registry.findByPath(process.cwd())?.slug;

			if (!resolvedSlug) {
				log.error("Not in a registered project. Use: graphmind export <slug> -f <format>");
				process.exitCode = 1;
				return;
			}

			const dbPath = graphDbPath(resolvedSlug);
			if (!existsSync(dbPath)) {
				log.error(`No graph for "${resolvedSlug}". Run: graphmind build`);
				process.exitCode = 1;
				return;
			}

			const db = initDatabase(dbPath);
			const edges = getFileEdges(db);
			db.close();

			if (edges.length === 0) {
				log.dim("No cross-file edges to export.");
				return;
			}

			const title = resolvedSlug;
			const output = format === "dot" ? exportDot(edges, title)
				: format === "mermaid" ? exportMermaid(edges, title)
				: exportJson(edges, title);
			console.log(output);
		});
}

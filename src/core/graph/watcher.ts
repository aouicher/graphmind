import { writeFileSync } from "node:fs";
import { watch } from "chokidar";
import { log } from "../../utils/logger.js";
import { metaPath } from "../../utils/paths.js";
import { Registry } from "../registry.js";
import { GraphBuilder } from "./builder.js";

const SUPPORTED_EXTENSIONS = [".ts", ".tsx", ".js", ".jsx", ".mjs"];
const DEBOUNCE_MS = 2000;

export function startWatcher(slug: string): void {
	const registry = new Registry();
	const project = registry.get(slug);
	if (!project) {
		log.error(`Project "${slug}" not found.`);
		process.exitCode = 1;
		return;
	}

	const ignored = [/node_modules/, /\.git/, /dist/, /build/, /coverage/, /__pycache__/, /\.next/];

	let debounceTimer: ReturnType<typeof setTimeout> | null = null;
	const pendingFiles = new Set<string>();

	const watcher = watch(project.path, {
		ignored,
		persistent: true,
		ignoreInitial: true,
	});

	async function rebuild() {
		const changed = pendingFiles.size;
		pendingFiles.clear();

		log.info(`${changed} file(s) changed, rebuilding "${slug}"...`);

		const builder = new GraphBuilder(slug);
		try {
			const result = await builder.build(project!.path, {
				full: false,
				exclude: project!.exclude,
			});

			registry.updateProject(slug, {
				lastBuild: new Date().toISOString(),
			});

			const meta = {
				lastBuild: new Date().toISOString(),
				filesProcessed: result.filesProcessed,
				symbolsFound: result.symbolsFound,
				edgesCreated: result.edgesCreated,
				duration: result.duration,
			};
			writeFileSync(metaPath(slug), JSON.stringify(meta, null, 2));

			log.success(
				`${result.symbolsFound} symbols, ${result.edgesCreated} edges (${result.duration}ms)`,
			);
		} catch (e) {
			log.error(`Rebuild failed: ${(e as Error).message}`);
		} finally {
			builder.close();
		}
	}

	function onFileChange(path: string) {
		const ext = path.slice(path.lastIndexOf("."));
		if (!SUPPORTED_EXTENSIONS.includes(ext)) return;

		pendingFiles.add(path);

		if (debounceTimer) clearTimeout(debounceTimer);
		debounceTimer = setTimeout(rebuild, DEBOUNCE_MS);
	}

	watcher.on("change", onFileChange);
	watcher.on("add", onFileChange);
	watcher.on("unlink", onFileChange);

	log.info(`Watching "${slug}" (${project.path}) — Ctrl+C to stop`);

	process.on("SIGINT", () => {
		watcher.close();
		process.exit(0);
	});
}

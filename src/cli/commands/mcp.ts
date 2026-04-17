import { copyFileSync, existsSync, mkdirSync } from "node:fs";
import { createRequire } from "node:module";
import { homedir } from "node:os";
import { dirname, join } from "node:path";
import type { Command } from "commander";
import { log } from "../../utils/logger.js";

export function registerMcpCommand(program: Command): void {
	program
		.command("mcp")
		.description("Start the MCP server")
		.option("--transport <type>", "Transport type (stdio|http)", "stdio")
		.option("--port <port>", "HTTP port (only with --transport http)", "37378")
		.option("--projects <slugs>", "Restrict to specific projects (comma-separated)")
		.action(async (opts: { transport: string; port: string; projects?: string }) => {
			if (opts.transport === "http") {
				const port = Number.parseInt(opts.port, 10);
				log.info(`Starting MCP server on http://127.0.0.1:${port}`);
			} else {
				log.info("Starting MCP server (stdio)");
			}

			const projectFilter = opts.projects?.split(",").map((s) => s.trim()) ?? null;

			const { startMcpServer } = await import("../../mcp/server.js");
			await startMcpServer({
				transport: opts.transport as "stdio" | "http",
				port: Number.parseInt(opts.port, 10),
				projects: projectFilter,
			});
		});

	program
		.command("install-skill")
		.description("Install the graphmind skill for Claude Code")
		.action(() => {
			const skillDir = join(homedir(), ".claude", "skills", "graphmind");
			mkdirSync(skillDir, { recursive: true });

			const require = createRequire(import.meta.url);
			const pkgRoot = dirname(require.resolve("../../package.json"));
			const sourcePath = join(pkgRoot, "src", "skill", "SKILL.md");
			const destPath = join(skillDir, "SKILL.md");

			if (existsSync(sourcePath)) {
				copyFileSync(sourcePath, destPath);
				log.success(`Skill installed at ${destPath}`);
			} else {
				log.error("SKILL.md not found in package. Reinstall graphmind.");
				process.exitCode = 1;
			}
		});
}

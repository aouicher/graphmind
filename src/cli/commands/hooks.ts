import { chmodSync, existsSync, mkdirSync, readFileSync, unlinkSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import type { Command } from "commander";
import { Registry } from "../../core/registry.js";
import { log } from "../../utils/logger.js";

const POST_COMMIT_HOOK = `#!/bin/sh
# graphmind: auto-rebuild graph after commit
graphmind build 2>/dev/null &
`;

const PRE_PUSH_HOOK = `#!/bin/sh
# graphmind: show diff-impact before push
echo ""
echo "  graphmind diff-impact:"
graphmind diff-impact 2>/dev/null
echo ""
`;

function installHook(hooksDir: string, name: string, content: string): boolean {
	const hookPath = join(hooksDir, name);

	if (existsSync(hookPath)) {
		const existing = readFileSync(hookPath, "utf-8");
		if (existing.includes("graphmind")) {
			log.dim(`  ${name}: already installed`);
			return false;
		}
		writeFileSync(hookPath, `${existing}\n${content}`);
	} else {
		writeFileSync(hookPath, content);
	}

	chmodSync(hookPath, 0o755);
	return true;
}

export function registerHooksCommand(program: Command): void {
	const hooks = program.command("hooks").description("Manage git hooks for graphmind");

	hooks
		.command("install [slug]")
		.description("Install git hooks (post-commit + pre-push)")
		.action((slug?: string) => {
			const registry = new Registry();
			const project = slug ? registry.get(slug) : registry.findByPath(process.cwd());

			if (!project) {
				log.error("Not in a registered project.");
				process.exitCode = 1;
				return;
			}

			const gitDir = join(project.path, ".git");
			if (!existsSync(gitDir)) {
				log.error(`No .git directory in ${project.path}`);
				process.exitCode = 1;
				return;
			}

			const hooksDir = join(gitDir, "hooks");
			mkdirSync(hooksDir, { recursive: true });

			let installed = 0;
			if (installHook(hooksDir, "post-commit", POST_COMMIT_HOOK)) installed++;
			if (installHook(hooksDir, "pre-push", PRE_PUSH_HOOK)) installed++;

			if (installed > 0) {
				log.success(`Installed ${installed} hook(s) for "${project.slug}"`);
			} else {
				log.dim("All hooks already installed.");
			}
		});

	hooks
		.command("uninstall [slug]")
		.description("Remove graphmind git hooks")
		.action((slug?: string) => {
			const registry = new Registry();
			const project = slug ? registry.get(slug) : registry.findByPath(process.cwd());

			if (!project) {
				log.error("Not in a registered project.");
				process.exitCode = 1;
				return;
			}

			const hooksDir = join(project.path, ".git", "hooks");
			let removed = 0;

			for (const name of ["post-commit", "pre-push"]) {
				const hookPath = join(hooksDir, name);
				if (!existsSync(hookPath)) continue;

				const content = readFileSync(hookPath, "utf-8");
				if (!content.includes("graphmind")) continue;

				const lines = content.split("\n");
				const filtered = lines.filter((l) => !l.includes("graphmind"));
				const remaining = filtered.filter((l) => l.trim() && l.trim() !== "#!/bin/sh").length;

				if (remaining === 0) {
					unlinkSync(hookPath);
				} else {
					writeFileSync(hookPath, filtered.join("\n"));
				}
				removed++;
			}

			if (removed > 0) {
				log.success(`Removed graphmind hooks from "${project.slug}"`);
			} else {
				log.dim("No graphmind hooks found.");
			}
		});
}

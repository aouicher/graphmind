import { cpSync, existsSync, readFileSync, readdirSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { defineConfig } from "tsup";

export default defineConfig({
	entry: {
		"cli/index": "src/cli/index.ts",
		index: "src/index.ts",
	},
	format: ["esm"],
	target: "node22",
	external: ["node:sqlite"],
	dts: true,
	clean: true,
	sourcemap: true,
	splitting: true,
	shims: true,
	banner: {
		js: "#!/usr/bin/env node\nprocess.removeAllListeners('warning');",
	},
	onSuccess: async () => {
		for (const file of readdirSync("dist")) {
			if (!file.endsWith(".js")) continue;
			const p = join("dist", file);
			const src = readFileSync(p, "utf-8");
			if (src.includes('from "sqlite"')) {
				writeFileSync(p, src.replace(/from "sqlite"/g, 'from "node:sqlite"'));
			}
		}
		const nativeDir = "src/native";
		if (existsSync(nativeDir)) {
			for (const file of readdirSync(nativeDir)) {
				if (file.endsWith(".node")) {
					cpSync(join(nativeDir, file), join("dist", file));
				}
			}
		}
	},
});

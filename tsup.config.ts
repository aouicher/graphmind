import { cpSync, existsSync, readdirSync } from "node:fs";
import { join } from "node:path";
import { defineConfig } from "tsup";

export default defineConfig({
	entry: {
		"cli/index": "src/cli/index.ts",
		index: "src/index.ts",
	},
	format: ["esm"],
	target: "node20",
	dts: true,
	clean: true,
	sourcemap: true,
	splitting: true,
	shims: true,
	banner: {
		js: "#!/usr/bin/env node",
	},
	onSuccess: async () => {
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

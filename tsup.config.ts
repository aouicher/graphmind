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
});

import { createRequire } from "node:module";
import { arch, platform } from "node:os";
import { join, dirname } from "node:path";
import { fileURLToPath } from "node:url";

const __dirname = dirname(fileURLToPath(import.meta.url));
const require = createRequire(import.meta.url);

const nodeName = `index.${platform()}-${arch()}.node`;
const nodePath = join(__dirname, nodeName);

let nativeModule;
try {
	nativeModule = require(nodePath);
} catch {
	nativeModule = null;
}

export const parseFile = nativeModule?.parseFile ?? null;
export const parseFiles = nativeModule?.parseFiles ?? null;
export const supportedLanguages = nativeModule?.supportedLanguages ?? null;
export const available = nativeModule !== null;

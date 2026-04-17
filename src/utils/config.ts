import { existsSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import {
	configPath,
	crossLinksDir,
	graphmindDir,
	graphsDir,
	memoryDir,
	sessionsDir,
} from "./paths.js";

export interface ProjectConfig {
	path: string;
	slug: string;
	registered: string;
	lastBuild: string | null;
	autoWatch: boolean;
	languages: string[];
	exclude: string[];
}

export interface GlobalConfig {
	version: string;
	projects: Record<string, ProjectConfig>;
	defaults: {
		embeddingModel: string;
		watchDebounce: number;
		maxDepth: number;
		excludeTests: boolean;
	};
	mcp: {
		transport: "stdio" | "http";
		httpPort: number;
		restrictToProjects: string[] | null;
	};
}

const DEFAULT_CONFIG: GlobalConfig = {
	version: "1",
	projects: {},
	defaults: {
		embeddingModel: "minilm",
		watchDebounce: 2000,
		maxDepth: 5,
		excludeTests: true,
	},
	mcp: {
		transport: "stdio",
		httpPort: 37378,
		restrictToProjects: null,
	},
};

export function ensureDirs(): void {
	for (const dir of [graphmindDir(), memoryDir(), graphsDir(), crossLinksDir(), sessionsDir()]) {
		mkdirSync(dir, { recursive: true });
	}
}

export function loadConfig(): GlobalConfig {
	ensureDirs();
	const cp = configPath();
	if (!existsSync(cp)) {
		writeFileSync(cp, JSON.stringify(DEFAULT_CONFIG, null, 2));
		return { ...DEFAULT_CONFIG };
	}
	const raw = readFileSync(cp, "utf-8");
	return JSON.parse(raw) as GlobalConfig;
}

export function saveConfig(config: GlobalConfig): void {
	ensureDirs();
	writeFileSync(configPath(), JSON.stringify(config, null, 2));
}

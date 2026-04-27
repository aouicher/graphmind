import { existsSync } from "node:fs";
import { basename, resolve } from "node:path";
import { type GlobalConfig, type ProjectConfig, loadConfig, saveConfig } from "../utils/config.js";

export class Registry {
	private config: GlobalConfig;

	constructor() {
		this.config = loadConfig();
	}

	register(path: string, options?: { slug?: string; exclude?: string[] }): ProjectConfig {
		const resolved = resolve(path);
		if (!existsSync(resolved)) {
			throw new Error(`Path does not exist: ${resolved}`);
		}

		const slug = options?.slug ?? this.slugify(basename(resolved));
		if (this.config.projects[slug]) {
			throw new Error(`Project "${slug}" is already registered. Use a different slug.`);
		}

		const project: ProjectConfig = {
			path: resolved,
			slug,
			registered: new Date().toISOString(),
			lastBuild: null,
			autoWatch: false,
			languages: [],
			exclude: options?.exclude ?? ["dist/**", "node_modules/**", "**/*.test.ts", "**/*.spec.ts"],
		};

		this.config.projects[slug] = project;
		saveConfig(this.config);
		return project;
	}

	unregister(slug: string): void {
		if (!this.config.projects[slug]) {
			throw new Error(`Project "${slug}" is not registered.`);
		}
		delete this.config.projects[slug];
		saveConfig(this.config);
	}

	get(slug: string): ProjectConfig | undefined {
		return this.config.projects[slug];
	}

	list(): ProjectConfig[] {
		return Object.values(this.config.projects);
	}

	findByPath(path: string): ProjectConfig | undefined {
		const resolved = resolve(path);
		return Object.values(this.config.projects)
			.filter((p) => resolved === p.path || resolved.startsWith(`${p.path}/`))
			.sort((a, b) => b.path.length - a.path.length)[0];
	}

	reload(): void {
		this.config = loadConfig();
	}

	getConfig(): GlobalConfig {
		return this.config;
	}

	updateProject(slug: string, updates: Partial<ProjectConfig>): void {
		const project = this.config.projects[slug];
		if (!project) {
			throw new Error(`Project "${slug}" is not registered.`);
		}
		Object.assign(project, updates);
		saveConfig(this.config);
	}

	private slugify(name: string): string {
		return name
			.toLowerCase()
			.replace(/[^a-z0-9-]/g, "-")
			.replace(/-+/g, "-")
			.replace(/^-|-$/g, "");
	}
}

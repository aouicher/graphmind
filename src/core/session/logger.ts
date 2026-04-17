import { appendFileSync, existsSync, mkdirSync, readFileSync, readdirSync } from "node:fs";
import { sessionLogPath, sessionsDir } from "../../utils/paths.js";

export class SessionLogger {
	constructor() {
		mkdirSync(sessionsDir(), { recursive: true });
	}

	start(slug: string): string {
		const date = new Date().toISOString().slice(0, 10);
		const time = new Date().toISOString().slice(11, 16);
		const logPath = sessionLogPath(date);
		const entry = `## ${time} — ${slug}\n\nSession started.\n\n`;
		appendFileSync(logPath, entry);
		return logPath;
	}

	save(slug: string, message: string): string {
		const date = new Date().toISOString().slice(0, 10);
		const time = new Date().toISOString().slice(11, 16);
		const logPath = sessionLogPath(date);
		const entry = `## ${time} — ${slug}\n\n${message}\n\n`;
		appendFileSync(logPath, entry);
		return logPath;
	}

	history(slug?: string, limit = 10): string[] {
		const entries: string[] = [];
		const dir = sessionsDir();
		if (!existsSync(dir)) return entries;

		const files = readdirSync(dir)
			.filter((f: string) => f.endsWith(".md"))
			.sort()
			.reverse();

		for (const file of files) {
			if (entries.length >= limit) break;
			const content = readFileSync(sessionLogPath(file.replace(".md", "")), "utf-8");
			const sections = content.split(/^## /m).filter(Boolean);

			for (const section of sections) {
				if (entries.length >= limit) break;
				if (slug && !section.includes(slug)) continue;
				entries.push(`## ${section.trim()}`);
			}
		}

		return entries;
	}
}

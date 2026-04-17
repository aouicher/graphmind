interface MdSymbol {
	name: string;
	kind: string;
	lineStart: number;
	lineEnd: number;
	signature: string | null;
	doc: string | null;
}

interface MdImport {
	source: string;
	specifiers: string[];
	line: number;
	isDefault: boolean;
	fromFile: string;
}

export interface MarkdownParseResult {
	path: string;
	language: string;
	symbols: MdSymbol[];
	callSites: never[];
	imports: MdImport[];
}

export function parseMarkdown(path: string, source: string): MarkdownParseResult {
	const lines = source.split("\n");
	const symbols: MdSymbol[] = [];
	const imports: MdImport[] = [];
	const seenLinks = new Set<string>();

	let inCodeBlock = false;
	let codeBlockStart = 0;
	let codeBlockLang = "";
	let codeBlockName = "";

	for (let i = 0; i < lines.length; i++) {
		const line = lines[i] ?? "";
		const lineNum = i + 1;

		if (line.startsWith("```")) {
			if (!inCodeBlock) {
				inCodeBlock = true;
				codeBlockStart = lineNum;
				codeBlockLang = line.slice(3).trim().split(/\s/)[0] ?? "";
				const prev = i > 0 ? (lines[i - 1] ?? "") : "";
				const headerMatch = prev.match(/^#+\s+(.+)/);
				codeBlockName = headerMatch?.[1] ?? `code-block-L${lineNum}`;
			} else {
				inCodeBlock = false;
				if (codeBlockLang) {
					symbols.push({
						name: codeBlockName,
						kind: "function",
						lineStart: codeBlockStart,
						lineEnd: lineNum,
						signature: codeBlockLang,
						doc: null,
					});
				}
			}
			continue;
		}

		if (inCodeBlock) continue;

		const headerMatch = line.match(/^(#{1,6})\s+(.+)/);
		if (headerMatch) {
			const hashes = headerMatch[1] ?? "";
			const level = hashes.length;
			const title = (headerMatch[2] ?? "").trim();
			let end = lineNum;
			for (let j = i + 1; j < lines.length; j++) {
				const nextLine = lines[j] ?? "";
				const nextHeader = nextLine.match(/^(#{1,6})\s/);
				if (nextHeader && (nextHeader[1] ?? "").length <= level) break;
				end = j + 1;
			}
			symbols.push({
				name: title,
				kind: level <= 2 ? "class" : "type",
				lineStart: lineNum,
				lineEnd: end,
				signature: `h${level}`,
				doc: null,
			});
		}

		const linkRegex = /\[([^\]]+)\]\(([^)]+)\)/g;
		let linkMatch: RegExpExecArray | null;
		// biome-ignore lint/suspicious/noAssignInExpressions: regex iteration
		while ((linkMatch = linkRegex.exec(line)) !== null) {
			const target = linkMatch[2] ?? "";
			if (seenLinks.has(target)) continue;
			seenLinks.add(target);

			if (target.startsWith("http://") || target.startsWith("https://")) continue;

			imports.push({
				source: target,
				specifiers: [linkMatch[1] ?? ""],
				line: lineNum,
				isDefault: true,
				fromFile: path,
			});
		}

		const wikilinkRegex = /\[\[([^\]|]+)(?:\|[^\]]+)?\]\]/g;
		let wikiMatch: RegExpExecArray | null;
		// biome-ignore lint/suspicious/noAssignInExpressions: regex iteration
		while ((wikiMatch = wikilinkRegex.exec(line)) !== null) {
			const target = wikiMatch[1] ?? "";
			if (seenLinks.has(target)) continue;
			seenLinks.add(target);

			imports.push({
				source: target,
				specifiers: [target],
				line: lineNum,
				isDefault: true,
				fromFile: path,
			});
		}
	}

	return {
		path,
		language: "markdown",
		symbols,
		callSites: [],
		imports,
	};
}

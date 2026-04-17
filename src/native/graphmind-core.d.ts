export interface ParsedFile {
	path: string;
	language: string;
	symbols: Array<{
		name: string;
		kind: string;
		line_start: number;
		line_end: number;
		signature?: string;
		doc?: string;
	}>;
	call_sites: Array<{
		caller: string;
		callee: string;
		line: number;
	}>;
	imports: Array<{
		source: string;
		specifiers: string[];
		line: number;
		is_default: boolean;
		from_file: string;
	}>;
}

export interface FileInput {
	path: string;
	source: string;
	language: string;
}

export function parseFile(path: string, source: string, language: string): ParsedFile;
export function parseFiles(files: FileInput[]): ParsedFile[];
export function supportedLanguages(): string[];

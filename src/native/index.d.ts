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

export declare const parseFile: ((path: string, source: string, language: string) => ParsedFile) | null;
export declare const parseFiles: ((files: Array<{ path: string; source: string; language: string }>) => ParsedFile[]) | null;
export declare const supportedLanguages: (() => string[]) | null;
export declare const available: boolean;

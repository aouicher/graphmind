import { mkdirSync } from "node:fs";
import { join } from "node:path";
import { DatabaseSync } from "node:sqlite";
import { graphDir } from "../../utils/paths.js";

export interface EmbeddingRow {
	id: number;
	symbol_name: string;
	symbol_kind: string;
	file: string;
	text: string;
	embedding: Buffer;
}

function initEmbeddingsDb(dbPath: string): DatabaseSync {
	mkdirSync(join(dbPath, ".."), { recursive: true });
	const db = new DatabaseSync(dbPath);
	db.exec("PRAGMA journal_mode = WAL");
	db.exec(`
		CREATE TABLE IF NOT EXISTS embeddings (
			id INTEGER PRIMARY KEY AUTOINCREMENT,
			symbol_name TEXT NOT NULL,
			symbol_kind TEXT NOT NULL,
			file TEXT NOT NULL,
			text TEXT NOT NULL,
			embedding BLOB NOT NULL
		);
		CREATE INDEX IF NOT EXISTS idx_embeddings_file ON embeddings(file);
	`);
	return db;
}

export class EmbeddingStore {
	private db: DatabaseSync;

	constructor(slug: string) {
		const dbPath = join(graphDir(slug), "embeddings.db");
		this.db = initEmbeddingsDb(dbPath);
	}

	clear(): void {
		this.db.exec("DELETE FROM embeddings");
	}

	insert(row: Omit<EmbeddingRow, "id">): void {
		this.db
			.prepare(
				"INSERT INTO embeddings (symbol_name, symbol_kind, file, text, embedding) VALUES (?, ?, ?, ?, ?)",
			)
			.run(row.symbol_name, row.symbol_kind, row.file, row.text, row.embedding);
	}

	insertBatch(rows: Array<Omit<EmbeddingRow, "id">>): void {
		const stmt = this.db.prepare(
			"INSERT INTO embeddings (symbol_name, symbol_kind, file, text, embedding) VALUES (?, ?, ?, ?, ?)",
		);
		this.db.exec("BEGIN TRANSACTION");
		try {
			for (const row of rows) {
				stmt.run(row.symbol_name, row.symbol_kind, row.file, row.text, row.embedding);
			}
			this.db.exec("COMMIT");
		} catch (e) {
			this.db.exec("ROLLBACK");
			throw e;
		}
	}

	all(): EmbeddingRow[] {
		return this.db.prepare("SELECT * FROM embeddings").all() as unknown as EmbeddingRow[];
	}

	count(): number {
		return (this.db.prepare("SELECT COUNT(*) as cnt FROM embeddings").get() as unknown as { cnt: number }).cnt;
	}

	close(): void {
		this.db.close();
	}
}

export function float32ToBuffer(arr: Float32Array): Buffer {
	return Buffer.from(arr.buffer, arr.byteOffset, arr.byteLength);
}

export function bufferToFloat32(buf: Buffer): Float32Array {
	const ab = buf.buffer.slice(buf.byteOffset, buf.byteOffset + buf.byteLength);
	return new Float32Array(ab);
}

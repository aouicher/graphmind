use rusqlite::{Connection, params};
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmbeddingRow {
    pub id: i64,
    pub symbol_name: String,
    pub symbol_kind: String,
    pub file: String,
    pub text: String,
    pub embedding: Vec<u8>,
}

pub struct EmbeddingStore {
    conn: Connection,
}

impl EmbeddingStore {
    pub fn open(db_path: &Path) -> rusqlite::Result<Self> {
        if let Some(parent) = db_path.parent() {
            std::fs::create_dir_all(parent).ok();
        }
        let conn = Connection::open(db_path)?;
        conn.execute_batch("PRAGMA journal_mode = WAL")?;
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS embeddings (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                symbol_name TEXT NOT NULL,
                symbol_kind TEXT NOT NULL,
                file TEXT NOT NULL,
                text TEXT NOT NULL,
                embedding BLOB NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_embeddings_file ON embeddings(file);",
        )?;
        Ok(Self { conn })
    }

    pub fn clear(&self) -> rusqlite::Result<()> {
        self.conn.execute("DELETE FROM embeddings", [])?;
        Ok(())
    }

    pub fn insert(
        &self,
        symbol_name: &str,
        symbol_kind: &str,
        file: &str,
        text: &str,
        embedding: &[u8],
    ) -> rusqlite::Result<()> {
        self.conn.execute(
            "INSERT INTO embeddings (symbol_name, symbol_kind, file, text, embedding) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![symbol_name, symbol_kind, file, text, embedding],
        )?;
        Ok(())
    }

    pub fn insert_batch(&self, rows: &[NewEmbeddingRow]) -> rusqlite::Result<()> {
        let tx = self.conn.unchecked_transaction()?;
        {
            let mut stmt = tx.prepare(
                "INSERT INTO embeddings (symbol_name, symbol_kind, file, text, embedding) VALUES (?1, ?2, ?3, ?4, ?5)",
            )?;
            for row in rows {
                stmt.execute(params![
                    row.symbol_name,
                    row.symbol_kind,
                    row.file,
                    row.text,
                    row.embedding,
                ])?;
            }
        }
        tx.commit()
    }

    pub fn all(&self) -> rusqlite::Result<Vec<EmbeddingRow>> {
        let mut stmt = self.conn.prepare("SELECT id, symbol_name, symbol_kind, file, text, embedding FROM embeddings")?;
        let rows = stmt.query_map([], |row| {
            Ok(EmbeddingRow {
                id: row.get(0)?,
                symbol_name: row.get(1)?,
                symbol_kind: row.get(2)?,
                file: row.get(3)?,
                text: row.get(4)?,
                embedding: row.get(5)?,
            })
        })?;
        rows.collect()
    }

    pub fn count(&self) -> rusqlite::Result<usize> {
        self.conn.query_row("SELECT COUNT(*) FROM embeddings", [], |r| r.get(0))
    }
}

pub struct NewEmbeddingRow {
    pub symbol_name: String,
    pub symbol_kind: String,
    pub file: String,
    pub text: String,
    pub embedding: Vec<u8>,
}

pub fn float32_to_bytes(arr: &[f32]) -> Vec<u8> {
    arr.iter().flat_map(|f| f.to_le_bytes()).collect()
}

pub fn bytes_to_float32(buf: &[u8]) -> Vec<f32> {
    buf.chunks_exact(4)
        .map(|chunk| f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]))
        .collect()
}

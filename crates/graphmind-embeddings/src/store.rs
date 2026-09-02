use rusqlite::{Connection, params};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
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

pub struct NewEmbeddingRow {
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
            CREATE INDEX IF NOT EXISTS idx_embeddings_file ON embeddings(file);
            CREATE TABLE IF NOT EXISTS embedding_meta (
                key TEXT PRIMARY KEY,
                value TEXT NOT NULL
            );",
        )?;
        Ok(Self { conn })
    }

    pub fn get_meta(&self, key: &str) -> Option<String> {
        self.conn
            .query_row(
                "SELECT value FROM embedding_meta WHERE key = ?1",
                params![key],
                |row| row.get(0),
            )
            .ok()
    }

    pub fn set_meta(&self, key: &str, value: &str) -> rusqlite::Result<()> {
        self.conn.execute(
            "INSERT OR REPLACE INTO embedding_meta (key, value) VALUES (?1, ?2)",
            params![key, value],
        )?;
        Ok(())
    }

    pub fn clear(&self) -> rusqlite::Result<()> {
        self.conn.execute("DELETE FROM embeddings", [])?;
        self.conn.execute("DELETE FROM embedding_meta", [])?;
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

    pub fn delete_by_file(&self, file: &str) -> rusqlite::Result<usize> {
        self.conn.execute("DELETE FROM embeddings WHERE file = ?1", params![file])
    }

    pub fn files_indexed(&self) -> HashSet<String> {
        let Ok(mut stmt) = self.conn.prepare("SELECT DISTINCT file FROM embeddings") else {
            return HashSet::new();
        };
        let Ok(rows) = stmt.query_map([], |row| row.get::<_, String>(0)) else {
            return HashSet::new();
        };
        rows.filter_map(|r| r.ok()).collect()
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

pub fn float32_to_bytes(arr: &[f32]) -> Vec<u8> {
    arr.iter().flat_map(|f| f.to_le_bytes()).collect()
}

pub fn bytes_to_float32(buf: &[u8]) -> Vec<f32> {
    // as_chunks (over chunks_exact) gives fixed-size arrays, so from_le_bytes
    // needs no per-element indexing. Trailing bytes are dropped, as before.
    buf.as_chunks::<4>()
        .0
        .iter()
        .map(|chunk| f32::from_le_bytes(*chunk))
        .collect()
}

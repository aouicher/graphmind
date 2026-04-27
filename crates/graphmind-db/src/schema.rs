use rusqlite::Connection;

pub const SCHEMA_SQL: &str = "
CREATE TABLE IF NOT EXISTS files (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  path TEXT UNIQUE NOT NULL,
  language TEXT,
  hash TEXT,
  last_parsed INTEGER
);

CREATE TABLE IF NOT EXISTS symbols (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  name TEXT NOT NULL,
  kind TEXT NOT NULL,
  file TEXT NOT NULL,
  line_start INTEGER,
  line_end INTEGER,
  signature TEXT,
  doc TEXT,
  content TEXT
);

CREATE TABLE IF NOT EXISTS edges (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  from_id INTEGER REFERENCES symbols(id) ON DELETE CASCADE,
  to_id INTEGER REFERENCES symbols(id) ON DELETE CASCADE,
  kind TEXT NOT NULL,
  confidence REAL DEFAULT 1.0,
  file TEXT
);

CREATE INDEX IF NOT EXISTS idx_symbols_name ON symbols(name);
CREATE INDEX IF NOT EXISTS idx_symbols_file ON symbols(file);
CREATE INDEX IF NOT EXISTS idx_edges_from ON edges(from_id);
CREATE INDEX IF NOT EXISTS idx_edges_to ON edges(to_id);
CREATE INDEX IF NOT EXISTS idx_files_path ON files(path);
";

const FTS_SQL: &str = "
CREATE VIRTUAL TABLE IF NOT EXISTS symbols_fts USING fts5(name, signature, doc, content_text, content=symbols, content_rowid=id);

CREATE TRIGGER IF NOT EXISTS symbols_ai AFTER INSERT ON symbols BEGIN
  INSERT INTO symbols_fts(rowid, name, signature, doc, content_text) VALUES (new.id, new.name, new.signature, new.doc, new.content);
END;

CREATE TRIGGER IF NOT EXISTS symbols_ad AFTER DELETE ON symbols BEGIN
  INSERT INTO symbols_fts(symbols_fts, rowid, name, signature, doc, content_text) VALUES('delete', old.id, old.name, old.signature, old.doc, old.content);
END;

CREATE TRIGGER IF NOT EXISTS symbols_au AFTER UPDATE ON symbols BEGIN
  INSERT INTO symbols_fts(symbols_fts, rowid, name, signature, doc, content_text) VALUES('delete', old.id, old.name, old.signature, old.doc, old.content);
  INSERT INTO symbols_fts(rowid, name, signature, doc, content_text) VALUES (new.id, new.name, new.signature, new.doc, new.content);
END;
";

pub fn init_database(db_path: &str) -> rusqlite::Result<Connection> {
    let db = Connection::open(db_path)?;
    db.execute_batch("PRAGMA journal_mode = WAL;")?;
    db.execute_batch("PRAGMA foreign_keys = ON;")?;
    db.execute_batch(SCHEMA_SQL)?;

    let has_content: bool = db
        .prepare("PRAGMA table_info(symbols)")?
        .query_map([], |row| row.get::<_, String>(1))?
        .any(|col| col.as_deref() == Ok("content"));

    if !has_content {
        db.execute_batch("ALTER TABLE symbols ADD COLUMN content TEXT;")?;
        db.execute_batch("DROP TABLE IF EXISTS symbols_fts;")?;
        db.execute_batch("DROP TRIGGER IF EXISTS symbols_ai;")?;
        db.execute_batch("DROP TRIGGER IF EXISTS symbols_ad;")?;
        db.execute_batch("DROP TRIGGER IF EXISTS symbols_au;")?;
    }

    db.execute_batch(FTS_SQL)?;
    Ok(db)
}

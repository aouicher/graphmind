use rusqlite::{Connection, Transaction, TransactionBehavior};

/// Version of the indexed content, stored in `meta.schema_version`: bump it when parsing output
/// changes so that existing graphs need `build --reset` to be correct. It says nothing about the
/// tables themselves; that is `LAYOUT_VERSION` (`PRAGMA user_version`), which upgrades in place.
pub const SCHEMA_VERSION: u32 = 2; // v0.2.134: edge kind classification

pub const SCHEMA_SQL: &str = "
CREATE TABLE IF NOT EXISTS meta (
  key TEXT PRIMARY KEY,
  value TEXT NOT NULL
);

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

/// Version of the database layout: the tables, indexes, FTS index, triggers and migrations
/// below. Stored in SQLite's `PRAGMA user_version` (file header, read without a lock). Bump it
/// with any change to `SCHEMA_SQL`, `FTS_SQL` or `migrate`. Not to be confused with
/// `meta.schema_version`, the version of the indexed content (see `stamp_schema_version`).
const LAYOUT_VERSION: i32 = 1;

/// Open the graph database, creating or upgrading its layout if needed.
///
/// Every query command opens the database through here, often while a build holds the write
/// lock. Once the layout is current this only reads (`PRAGMA user_version`): no DDL and no meta
/// writes, so a reader never waits on (or fails behind) a running build. Existing databases have
/// `user_version = 0` and go through `migrate` once.
pub fn init_database(db_path: &str) -> rusqlite::Result<Connection> {
    let db = Connection::open(db_path)?;
    db.execute_batch("PRAGMA foreign_keys = ON;")?;
    let layout: i32 = db.query_row("PRAGMA user_version", [], |row| row.get(0))?;
    if layout < LAYOUT_VERSION {
        migrate(&db)?;
    }
    Ok(db)
}

/// Bring the layout to `LAYOUT_VERSION`: WAL mode (also for graphs seeded by 0.3.0-0.3.2 with
/// `VACUUM INTO`, which come out in rollback mode and keep `user_version` 0), the idempotent DDL
/// and the content-column migration. A new database gets its content version stamped.
///
/// WAL is set first (SQLite can't change the journal mode inside a transaction). Everything else
/// runs in one `BEGIN IMMEDIATE` transaction, so a crash or error leaves the old layout intact
/// rather than half-migrated, and a second process migrating at the same time waits for the lock,
/// then finds the layout current and does nothing.
fn migrate(db: &Connection) -> rusqlite::Result<()> {
    db.execute_batch("PRAGMA journal_mode = WAL;")?;
    let tx = Transaction::new_unchecked(db, TransactionBehavior::Immediate)?;
    let layout: i32 = tx.query_row("PRAGMA user_version", [], |row| row.get(0))?;
    if layout >= LAYOUT_VERSION {
        return tx.commit();
    }
    let fresh = !tx
        .prepare("SELECT 1 FROM sqlite_master WHERE name = 'symbols'")?
        .exists([])?;

    tx.execute_batch(SCHEMA_SQL)?;

    let has_content: bool = tx
        .prepare("PRAGMA table_info(symbols)")?
        .query_map([], |row| row.get::<_, String>(1))?
        .any(|col| col.as_deref() == Ok("content"));

    if !has_content {
        tx.execute_batch("ALTER TABLE symbols ADD COLUMN content TEXT;")?;
        tx.execute_batch("DROP TABLE IF EXISTS symbols_fts;")?;
        tx.execute_batch("DROP TRIGGER IF EXISTS symbols_ai;")?;
        tx.execute_batch("DROP TRIGGER IF EXISTS symbols_ad;")?;
        tx.execute_batch("DROP TRIGGER IF EXISTS symbols_au;")?;
    }

    tx.execute_batch(FTS_SQL)?;

    if fresh {
        stamp_schema_version(&tx)?;
    }
    tx.execute_batch(&format!("PRAGMA user_version = {LAYOUT_VERSION};"))?;
    tx.commit()
}

/// Record that this database's indexed content matches the current schema version. Only a new
/// database is stamped (`migrate`); `build --reset` deletes the database, so its rebuild starts
/// from a new, stamped one. Anything else keeps reporting an outdated index until a reset.
fn stamp_schema_version(db: &Connection) -> rusqlite::Result<()> {
    db.execute(
        "INSERT OR REPLACE INTO meta (key, value) VALUES ('schema_version', ?1)",
        rusqlite::params![SCHEMA_VERSION.to_string()],
    )?;
    Ok(())
}

/// Returns true if the DB at db_path needs a --reset (schema version is outdated).
pub fn schema_needs_reset(db_path: &str) -> bool {
    if !std::path::Path::new(db_path).exists() {
        return false;
    }
    read_schema_version(db_path) < SCHEMA_VERSION
}

/// Returns the schema version stored in the DB, or 1 if not present (pre-meta DBs).
pub fn read_schema_version(db_path: &str) -> u32 {
    let Ok(db) = Connection::open(db_path) else { return 1 };
    // meta table may not exist in old DBs
    let result = db.query_row(
        "SELECT value FROM meta WHERE key = 'schema_version'",
        [],
        |row| row.get::<_, String>(0),
    );
    result.ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(1)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn journal_mode(db: &Connection) -> String {
        db.query_row("PRAGMA journal_mode", [], |row| row.get(0)).unwrap()
    }

    /// A worktree graph seeded by 0.3.0-0.3.2 (`VACUUM INTO`) is in rollback mode with
    /// `user_version` 0: opening it migrates it once, which puts it in WAL.
    #[test]
    fn copy_seeded_by_an_older_version_is_switched_to_wal() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("graph.db");
        let copy = dir.path().join("copy.db");
        let db = init_database(src.to_str().unwrap()).unwrap();
        db.execute_batch("PRAGMA user_version = 0;").unwrap(); // as before layout versions
        db.execute("VACUUM INTO ?1", [copy.to_str().unwrap()]).unwrap();
        drop(db);
        assert_eq!(journal_mode(&Connection::open(&copy).unwrap()), "delete", "precondition");

        let opened = init_database(copy.to_str().unwrap()).unwrap();
        assert_eq!(journal_mode(&opened), "wal");
        let layout: i32 = opened.query_row("PRAGMA user_version", [], |r| r.get(0)).unwrap();
        assert_eq!(layout, LAYOUT_VERSION);
    }

    /// An existing database (`user_version` 0) goes through the DDL path once, keeps its data
    /// and its content version, and is then current.
    #[test]
    fn existing_database_is_migrated_once() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("graph.db");
        let path = path.to_str().unwrap();
        let db = init_database(path).unwrap();
        db.execute("INSERT INTO files (path, language, hash) VALUES ('a.rs', 'rust', 'h')", []).unwrap();
        db.execute("UPDATE meta SET value = '1' WHERE key = 'schema_version'", []).unwrap();
        db.execute_batch("PRAGMA user_version = 0;").unwrap();
        drop(db);

        let db = init_database(path).unwrap();
        let layout: i32 = db.query_row("PRAGMA user_version", [], |r| r.get(0)).unwrap();
        assert_eq!(layout, LAYOUT_VERSION);
        let files: i64 = db.query_row("SELECT COUNT(*) FROM files", [], |r| r.get(0)).unwrap();
        assert_eq!(files, 1, "data kept");
        assert!(schema_needs_reset(path), "migrating the layout doesn't claim the content is current");
    }

    #[test]
    fn new_database_is_stamped_and_readers_dont_restamp() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("graph.db");
        let path = path.to_str().unwrap();
        init_database(path).unwrap();
        assert_eq!(read_schema_version(path), SCHEMA_VERSION);

        // An index from an older version: a reader must not hide that it needs a reset.
        let db = Connection::open(path).unwrap();
        db.execute("UPDATE meta SET value = '1' WHERE key = 'schema_version'", []).unwrap();
        drop(db);
        init_database(path).unwrap();
        assert!(schema_needs_reset(path));
    }

    /// A migration that fails part-way is rolled back: no tables from the DDL that ran before the
    /// error, and `user_version` stays 0, so the next open tries again.
    #[test]
    fn failed_migration_leaves_the_database_unchanged() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("graph.db");
        let path = path.to_str().unwrap();
        // A `symbols` view: `SCHEMA_SQL` creates `meta` and `files`, then fails on its index.
        Connection::open(path).unwrap().execute_batch("CREATE VIEW symbols AS SELECT 1 AS id;").unwrap();

        assert!(init_database(path).is_err());
        let db = Connection::open(path).unwrap();
        let files = db.prepare("SELECT 1 FROM sqlite_master WHERE name = 'files'").unwrap().exists([]).unwrap();
        assert!(!files, "DDL before the error was rolled back");
        let layout: i32 = db.query_row("PRAGMA user_version", [], |r| r.get(0)).unwrap();
        assert_eq!(layout, 0);
    }

    /// Several processes opening a new database at once: each waits for the migration lock, and
    /// all of them succeed on a current, stamped layout.
    #[test]
    fn concurrent_first_opens_all_succeed() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("graph.db").to_str().unwrap().to_string();
        let handles: Vec<_> = (0..8)
            .map(|_| {
                let path = path.clone();
                std::thread::spawn(move || init_database(&path).map(|_| ()))
            })
            .collect();
        for h in handles {
            h.join().unwrap().expect("every first open succeeds");
        }
        assert_eq!(read_schema_version(&path), SCHEMA_VERSION);
    }

    #[test]
    fn reader_opens_while_a_writer_holds_the_lock() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("graph.db");
        let path = path.to_str().unwrap();
        init_database(path).unwrap();

        let writer = Connection::open(path).unwrap();
        writer.execute_batch("BEGIN IMMEDIATE; INSERT INTO files (path) VALUES ('a.rs');").unwrap();

        let start = std::time::Instant::now();
        let reader = init_database(path).expect("reader must not need the write lock");
        let n: i64 = reader.query_row("SELECT COUNT(*) FROM symbols", [], |r| r.get(0)).unwrap();
        assert_eq!(n, 0);
        assert!(start.elapsed() < std::time::Duration::from_secs(1), "reader waited {:?}", start.elapsed());
        writer.execute_batch("COMMIT").unwrap();
    }
}

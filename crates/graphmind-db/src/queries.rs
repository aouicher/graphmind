use rusqlite::{params, Connection};
use std::collections::HashSet;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SymbolRow {
    pub id: i64,
    pub name: String,
    pub kind: String,
    pub file: String,
    pub line_start: i64,
    pub line_end: i64,
    pub signature: Option<String>,
    pub doc: Option<String>,
    pub content: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SymbolWithEdge {
    pub id: i64,
    pub name: String,
    pub kind: String,
    pub file: String,
    pub line_start: i64,
    pub line_end: i64,
    pub signature: Option<String>,
    pub doc: Option<String>,
    pub content: Option<String>,
    pub edge_kind: String,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct FileDep {
    pub file: String,
    pub kind: String,
    pub count: i64,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct FileConnection {
    pub file: String,
    pub connections: i64,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CyclePair {
    pub from_file: String,
    pub to_file: String,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Stats {
    pub symbols: i64,
    pub edges: i64,
    pub files: i64,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct LanguageCount {
    pub language: String,
    pub count: i64,
}

pub struct GraphQueries<'a> {
    db: &'a Connection,
}

impl<'a> GraphQueries<'a> {
    pub fn new(db: &'a Connection) -> Self {
        Self { db }
    }

    pub fn find_symbol(&self, name: &str) -> Vec<SymbolRow> {
        self.find_symbol_filtered(name, None, None)
    }

    pub fn find_symbol_filtered(&self, name: &str, file: Option<&str>, kind: Option<&str>) -> Vec<SymbolRow> {
        let mut sql = String::from("SELECT id, name, kind, file, line_start, line_end, signature, doc, content FROM symbols WHERE name = ?1");
        let mut param_idx = 2;
        if file.is_some() {
            sql.push_str(&format!(" AND file = ?{param_idx}"));
            param_idx += 1;
        }
        if kind.is_some() {
            sql.push_str(&format!(" AND kind = ?{param_idx} COLLATE NOCASE"));
        }
        let mut stmt = self.db.prepare(&sql).unwrap();
        let mut params_vec: Vec<Box<dyn rusqlite::types::ToSql>> = vec![Box::new(name.to_string())];
        if let Some(f) = file {
            params_vec.push(Box::new(f.to_string()));
        }
        if let Some(k) = kind {
            params_vec.push(Box::new(k.to_string()));
        }
        let param_refs: Vec<&dyn rusqlite::types::ToSql> = params_vec.iter().map(|p| p.as_ref()).collect();
        stmt.query_map(param_refs.as_slice(), |row| {
            Ok(SymbolRow {
                id: row.get(0)?,
                name: row.get(1)?,
                kind: row.get(2)?,
                file: row.get(3)?,
                line_start: row.get(4)?,
                line_end: row.get(5)?,
                signature: row.get(6)?,
                doc: row.get(7)?,
                content: row.get(8)?,
            })
        })
        .unwrap()
        .filter_map(|r| r.ok())
        .collect()
    }

    pub fn search_symbols(&self, query: &str, limit: i64) -> Vec<SymbolRow> {
        let mut stmt = self.db.prepare(
            "SELECT s.id, s.name, s.kind, s.file, s.line_start, s.line_end, s.signature, s.doc, s.content
             FROM symbols_fts f
             JOIN symbols s ON s.id = f.rowid
             WHERE symbols_fts MATCH ?1
             ORDER BY bm25(symbols_fts, 10.0, 5.0, 3.0, 1.0)
             LIMIT ?2"
        ).unwrap();
        stmt.query_map(params![query, limit], |row| {
            Ok(SymbolRow {
                id: row.get(0)?,
                name: row.get(1)?,
                kind: row.get(2)?,
                file: row.get(3)?,
                line_start: row.get(4)?,
                line_end: row.get(5)?,
                signature: row.get(6)?,
                doc: row.get(7)?,
                content: row.get(8)?,
            })
        })
        .unwrap()
        .filter_map(|r| r.ok())
        .collect()
    }

    pub fn callers(&self, symbol_name: &str) -> Vec<SymbolWithEdge> {
        self.callers_filtered(symbol_name, None)
    }

    pub fn callers_filtered(&self, symbol_name: &str, file: Option<&str>) -> Vec<SymbolWithEdge> {
        let sql = if file.is_some() {
            "SELECT s.id, s.name, s.kind, s.file, s.line_start, s.line_end, s.signature, s.doc, s.content, e.kind as edge_kind
             FROM edges e
             JOIN symbols s ON s.id = e.from_id
             JOIN symbols target ON target.id = e.to_id
             WHERE target.name = ?1 AND target.file = ?2"
        } else {
            "SELECT s.id, s.name, s.kind, s.file, s.line_start, s.line_end, s.signature, s.doc, s.content, e.kind as edge_kind
             FROM edges e
             JOIN symbols s ON s.id = e.from_id
             JOIN symbols target ON target.id = e.to_id
             WHERE target.name = ?1"
        };
        let mut stmt = self.db.prepare(sql).unwrap();
        let map_row = |row: &rusqlite::Row| {
            Ok(SymbolWithEdge {
                id: row.get(0)?,
                name: row.get(1)?,
                kind: row.get(2)?,
                file: row.get(3)?,
                line_start: row.get(4)?,
                line_end: row.get(5)?,
                signature: row.get(6)?,
                doc: row.get(7)?,
                content: row.get(8)?,
                edge_kind: row.get(9)?,
            })
        };
        if let Some(f) = file {
            stmt.query_map(params![symbol_name, f], map_row)
        } else {
            stmt.query_map(params![symbol_name], map_row)
        }
        .unwrap()
        .filter_map(|r| r.ok())
        .collect()
    }

    pub fn callees(&self, symbol_name: &str) -> Vec<SymbolWithEdge> {
        self.callees_filtered(symbol_name, None)
    }

    pub fn callees_filtered(&self, symbol_name: &str, file: Option<&str>) -> Vec<SymbolWithEdge> {
        let sql = if file.is_some() {
            "SELECT s.id, s.name, s.kind, s.file, s.line_start, s.line_end, s.signature, s.doc, s.content, e.kind as edge_kind
             FROM edges e
             JOIN symbols s ON s.id = e.to_id
             JOIN symbols source ON source.id = e.from_id
             WHERE source.name = ?1 AND source.file = ?2"
        } else {
            "SELECT s.id, s.name, s.kind, s.file, s.line_start, s.line_end, s.signature, s.doc, s.content, e.kind as edge_kind
             FROM edges e
             JOIN symbols s ON s.id = e.to_id
             JOIN symbols source ON source.id = e.from_id
             WHERE source.name = ?1"
        };
        let mut stmt = self.db.prepare(sql).unwrap();
        let map_row = |row: &rusqlite::Row| {
            Ok(SymbolWithEdge {
                id: row.get(0)?,
                name: row.get(1)?,
                kind: row.get(2)?,
                file: row.get(3)?,
                line_start: row.get(4)?,
                line_end: row.get(5)?,
                signature: row.get(6)?,
                doc: row.get(7)?,
                content: row.get(8)?,
                edge_kind: row.get(9)?,
            })
        };
        if let Some(f) = file {
            stmt.query_map(params![symbol_name, f], map_row)
        } else {
            stmt.query_map(params![symbol_name], map_row)
        }
        .unwrap()
        .filter_map(|r| r.ok())
        .collect()
    }

    pub fn resolve_qualified_name(&self, sym: &SymbolRow) -> String {
        let mut stmt = self.db.prepare(
            "SELECT name, kind FROM symbols
             WHERE file = ?1
             AND kind IN ('Class', 'Interface', 'Module')
             AND line_start < ?2
             AND line_end > ?3
             AND id != ?4
             ORDER BY line_start ASC"
        ).unwrap();
        let parents: Vec<String> = stmt.query_map(
            params![sym.file, sym.line_start, sym.line_end, sym.id],
            |row| row.get::<_, String>(0),
        )
        .unwrap()
        .filter_map(|r| r.ok())
        .collect();
        if parents.is_empty() {
            return sym.name.clone();
        }
        let sep = if sym.kind == "Method" || sym.kind == "Function" { "#" } else { "::" };
        format!("{}{}{}",parents.join("::"), sep, sym.name)
    }

    pub fn find_listeners(&self, event_name: &str) -> Vec<SymbolRow> {
        let mut stmt = self.db.prepare(
            "SELECT id, name, kind, file, line_start, line_end, signature, doc, content
             FROM symbols
             WHERE name = ?1 AND kind IN ('Function', 'Method')"
        ).unwrap();
        stmt.query_map(params![event_name], |row| {
            Ok(SymbolRow {
                id: row.get(0)?,
                name: row.get(1)?,
                kind: row.get(2)?,
                file: row.get(3)?,
                line_start: row.get(4)?,
                line_end: row.get(5)?,
                signature: row.get(6)?,
                doc: row.get(7)?,
                content: row.get(8)?,
            })
        })
        .unwrap()
        .filter_map(|r| r.ok())
        .collect()
    }

    pub fn file_deps(&self, file_path: &str) -> Vec<FileDep> {
        let mut stmt = self.db.prepare(
            "SELECT target_s.file as file, e.kind, COUNT(*) as count
             FROM edges e
             JOIN symbols source_s ON source_s.id = e.from_id
             JOIN symbols target_s ON target_s.id = e.to_id
             WHERE source_s.file = ?1 AND target_s.file != ?1
             GROUP BY target_s.file, e.kind
             ORDER BY count DESC"
        ).unwrap();
        stmt.query_map(params![file_path], |row| {
            Ok(FileDep {
                file: row.get(0)?,
                kind: row.get(1)?,
                count: row.get(2)?,
            })
        })
        .unwrap()
        .filter_map(|r| r.ok())
        .collect()
    }

    pub fn file_reverse_deps(&self, file_path: &str) -> Vec<FileDep> {
        let mut stmt = self.db.prepare(
            "SELECT source_s.file as file, e.kind, COUNT(*) as count
             FROM edges e
             JOIN symbols source_s ON source_s.id = e.from_id
             JOIN symbols target_s ON target_s.id = e.to_id
             WHERE target_s.file = ?1 AND source_s.file != ?1
             GROUP BY source_s.file, e.kind
             ORDER BY count DESC"
        ).unwrap();
        stmt.query_map(params![file_path], |row| {
            Ok(FileDep {
                file: row.get(0)?,
                kind: row.get(1)?,
                count: row.get(2)?,
            })
        })
        .unwrap()
        .filter_map(|r| r.ok())
        .collect()
    }

    pub fn impact(&self, file_path: &str, max_depth: usize) -> Vec<String> {
        let mut visited = HashSet::new();
        visited.insert(file_path.to_string());
        let mut queue = vec![file_path.to_string()];
        let mut depth = 0;

        while !queue.is_empty() && depth < max_depth {
            let batch: Vec<String> = std::mem::take(&mut queue);
            for file in &batch {
                let deps = self.file_reverse_deps(file);
                for dep in deps {
                    if visited.insert(dep.file.clone()) {
                        queue.push(dep.file);
                    }
                }
            }
            depth += 1;
        }

        visited.remove(file_path);
        visited.into_iter().collect()
    }

    pub fn symbols_in_file(&self, file_path: &str) -> Vec<SymbolRow> {
        let mut stmt = self.db.prepare("SELECT id, name, kind, file, line_start, line_end, signature, doc, content FROM symbols WHERE file = ?1").unwrap();
        stmt.query_map(params![file_path], |row| {
            Ok(SymbolRow {
                id: row.get(0)?,
                name: row.get(1)?,
                kind: row.get(2)?,
                file: row.get(3)?,
                line_start: row.get(4)?,
                line_end: row.get(5)?,
                signature: row.get(6)?,
                doc: row.get(7)?,
                content: row.get(8)?,
            })
        })
        .unwrap()
        .filter_map(|r| r.ok())
        .collect()
    }

    pub fn top_connected(&self, limit: i64) -> Vec<FileConnection> {
        let mut stmt = self.db.prepare(
            "SELECT s.file, COUNT(DISTINCT e.id) as connections
             FROM symbols s
             LEFT JOIN edges e ON e.from_id = s.id OR e.to_id = s.id
             GROUP BY s.file
             ORDER BY connections DESC
             LIMIT ?1"
        ).unwrap();
        stmt.query_map(params![limit], |row| {
            Ok(FileConnection {
                file: row.get(0)?,
                connections: row.get(1)?,
            })
        })
        .unwrap()
        .filter_map(|r| r.ok())
        .collect()
    }

    pub fn detect_cycles(&self) -> Vec<CyclePair> {
        let mut stmt = self.db.prepare(
            "SELECT DISTINCT s1.file as from_file, s2.file as to_file
             FROM edges e1
             JOIN symbols s1 ON s1.id = e1.from_id
             JOIN symbols s2 ON s2.id = e1.to_id
             WHERE s1.file != s2.file
             AND EXISTS (
               SELECT 1 FROM edges e2
               JOIN symbols s3 ON s3.id = e2.from_id
               JOIN symbols s4 ON s4.id = e2.to_id
               WHERE s3.file = s2.file AND s4.file = s1.file
             )"
        ).unwrap();
        stmt.query_map([], |row| {
            Ok(CyclePair {
                from_file: row.get(0)?,
                to_file: row.get(1)?,
            })
        })
        .unwrap()
        .filter_map(|r| r.ok())
        .collect()
    }

    pub fn stats(&self) -> Stats {
        let symbols: i64 = self.db.query_row("SELECT COUNT(*) FROM symbols", [], |r| r.get(0)).unwrap_or(0);
        let edges: i64 = self.db.query_row("SELECT COUNT(*) FROM edges", [], |r| r.get(0)).unwrap_or(0);
        let files: i64 = self.db.query_row("SELECT COUNT(*) FROM files", [], |r| r.get(0)).unwrap_or(0);
        Stats { symbols, edges, files }
    }

    pub fn language_breakdown(&self) -> Vec<LanguageCount> {
        let mut stmt = self.db.prepare(
            "SELECT language, COUNT(*) as count FROM files WHERE language IS NOT NULL GROUP BY language ORDER BY count DESC"
        ).unwrap();
        stmt.query_map([], |row| {
            Ok(LanguageCount {
                language: row.get(0)?,
                count: row.get(1)?,
            })
        })
        .unwrap()
        .filter_map(|r| r.ok())
        .collect()
    }
}

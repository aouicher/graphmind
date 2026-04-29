use graphmind_config::{paths, Registry};
use graphmind_db::queries::GraphQueries;
use rusqlite::Connection;
use serde::Serialize;

#[derive(Serialize)]
pub struct GraphNode {
    pub id: String,
    pub name: String,
    pub kind: String,
    pub file: String,
    pub line_start: i64,
    pub connections: usize,
}

#[derive(Serialize)]
pub struct GraphEdge {
    pub source: String,
    pub target: String,
    pub kind: String,
}

#[derive(Serialize)]
pub struct GraphData {
    pub nodes: Vec<GraphNode>,
    pub edges: Vec<GraphEdge>,
    pub files: Vec<String>,
    pub kinds: Vec<String>,
    pub languages: Vec<String>,
}

#[tauri::command]
pub async fn get_graph_data(slug: String, file_filter: Option<String>, kind_filter: Option<String>, language_filter: Option<String>, limit: Option<i64>) -> Result<GraphData, String> {
    let _project = Registry::get(&slug).ok_or_else(|| format!("Project {slug} not found"))?;
    let db_path = paths::graph_db_path(&slug);

    if !db_path.exists() {
        return Err("Project not indexed yet. Build it first.".to_string());
    }

    let node_limit = limit.unwrap_or(1500);

    let result = tokio::task::spawn_blocking(move || {
        let db = Connection::open(&db_path).map_err(|e| format!("DB error: {e}"))?;
        let queries = GraphQueries::new(&db);

        let mut sql = String::from(
            "SELECT s.id, s.name, s.kind, s.file, s.line_start,
                    (SELECT COUNT(*) FROM edges e WHERE e.from_id = s.id OR e.to_id = s.id) as connections
             FROM symbols s WHERE 1=1"
        );
        if let Some(ref f) = file_filter {
            sql.push_str(&format!(" AND s.file LIKE '%{}%'", f.replace('\'', "''")));
        }
        if let Some(ref k) = kind_filter {
            sql.push_str(&format!(" AND s.kind = '{}'", k.replace('\'', "''")));
        }
        if let Some(ref lang) = language_filter {
            sql.push_str(&format!(
                " AND s.file IN (SELECT path FROM files WHERE language = '{}')",
                lang.replace('\'', "''")
            ));
        }
        sql.push_str(&format!(" ORDER BY connections DESC LIMIT {}", node_limit));

        let mut stmt = db.prepare(&sql).map_err(|e| format!("Query error: {e}"))?;
        let nodes: Vec<GraphNode> = stmt
            .query_map([], |row| {
                Ok(GraphNode {
                    id: format!("{}", row.get::<_, i64>(0)?),
                    name: row.get(1)?,
                    kind: row.get(2)?,
                    file: row.get(3)?,
                    line_start: row.get(4)?,
                    connections: row.get::<_, i64>(5)? as usize,
                })
            })
            .map_err(|e| format!("Query error: {e}"))?
            .filter_map(|r| r.ok())
            .collect();

        let node_ids: Vec<String> = nodes.iter().map(|n| n.id.clone()).collect();

        let edges = if !node_ids.is_empty() {
            let placeholders: Vec<String> = node_ids.to_vec();
            let id_set = placeholders.join(",");
            let edge_sql = format!(
                "SELECT e.from_id, e.to_id, e.kind FROM edges e
                 WHERE e.from_id IN ({}) AND e.to_id IN ({})",
                id_set, id_set
            );
            let mut edge_stmt = db.prepare(&edge_sql).map_err(|e| format!("Edge query error: {e}"))?;
            let rows: Vec<GraphEdge> = edge_stmt
                .query_map([], |row| {
                    Ok(GraphEdge {
                        source: format!("{}", row.get::<_, i64>(0)?),
                        target: format!("{}", row.get::<_, i64>(1)?),
                        kind: row.get(2)?,
                    })
                })
                .map_err(|e| format!("Edge query error: {e}"))?
                .filter_map(|r| r.ok())
                .collect();
            rows
        } else {
            Vec::new()
        };

        let files: Vec<String> = {
            let mut stmt = db.prepare("SELECT DISTINCT file FROM symbols ORDER BY file")
                .map_err(|e| format!("Files query error: {e}"))?;
            let rows: Vec<String> = stmt.query_map([], |row| row.get(0))
                .map_err(|e| format!("Files query error: {e}"))?
                .filter_map(|r| r.ok())
                .collect();
            rows
        };

        let kinds: Vec<String> = {
            let mut stmt = db.prepare("SELECT DISTINCT kind FROM symbols ORDER BY kind")
                .map_err(|e| format!("Kinds query error: {e}"))?;
            let rows: Vec<String> = stmt.query_map([], |row| row.get(0))
                .map_err(|e| format!("Kinds query error: {e}"))?
                .filter_map(|r| r.ok())
                .collect();
            rows
        };

        let languages: Vec<String> = {
            let mut stmt = db.prepare("SELECT DISTINCT language FROM files WHERE language IS NOT NULL ORDER BY language")
                .map_err(|e| format!("Languages query error: {e}"))?;
            let rows: Vec<String> = stmt.query_map([], |row| row.get(0))
                .map_err(|e| format!("Languages query error: {e}"))?
                .filter_map(|r| r.ok())
                .collect();
            rows
        };

        let _ = queries.stats();

        Ok::<GraphData, String>(GraphData { nodes, edges, files, kinds, languages })
    })
    .await
    .map_err(|e| format!("Task error: {e}"))??;

    Ok(result)
}

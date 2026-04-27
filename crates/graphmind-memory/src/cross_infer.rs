use crate::cross_links::{CrossLinkStore, LinkType, NewCrossLink, CrossLink};
use graphmind_db::schema::init_database;
use std::collections::{HashMap, HashSet};
use std::path::Path;

struct SharedSymbol {
    name: String,
    kind: String,
    projects: Vec<String>,
}

pub fn infer_cross_links(
    project_slugs: &[String],
    db_path_fn: impl Fn(&str) -> String,
    cross_links_store: &CrossLinkStore,
) -> Vec<CrossLink> {
    let existing = cross_links_store.list();
    let mut symbol_map: HashMap<String, HashSet<String>> = HashMap::new();

    for slug in project_slugs {
        let db_path = db_path_fn(slug);
        if !Path::new(&db_path).exists() {
            continue;
        }

        let db = match init_database(&db_path) {
            Ok(db) => db,
            Err(_) => continue,
        };

        let mut stmt = match db.prepare("SELECT DISTINCT name, kind FROM symbols") {
            Ok(s) => s,
            Err(_) => continue,
        };

        let symbols: Vec<(String, String)> = match stmt.query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        }) {
            Ok(rows) => rows.filter_map(|r| r.ok()).collect(),
            Err(_) => continue,
        };

        for (name, kind) in symbols {
            let key = format!("{name}:{kind}");
            symbol_map
                .entry(key)
                .or_default()
                .insert(slug.clone());
        }
    }

    let shared: Vec<SharedSymbol> = symbol_map
        .into_iter()
        .filter(|(_, projects)| projects.len() > 1)
        .map(|(key, projects)| {
            let mut parts = key.splitn(2, ':');
            SharedSymbol {
                name: parts.next().unwrap_or("").to_string(),
                kind: parts.next().unwrap_or("").to_string(),
                projects: projects.into_iter().collect(),
            }
        })
        .collect();

    let mut link_pairs: HashSet<String> = existing
        .iter()
        .map(|l| format!("{}:{}", l.from, l.to))
        .collect();

    let mut new_links = Vec::new();

    for sym in &shared {
        for i in 0..sym.projects.len() {
            for j in (i + 1)..sym.projects.len() {
                let from = &sym.projects[i];
                let to = &sym.projects[j];
                let pair_key = format!("{from}:{to}");
                let reverse_key = format!("{to}:{from}");

                if link_pairs.contains(&pair_key) || link_pairs.contains(&reverse_key) {
                    continue;
                }

                let link = cross_links_store.add(NewCrossLink {
                    from: from.clone(),
                    to: to.clone(),
                    link_type: LinkType::SharesPattern,
                    reason: format!("Shared symbol: {} ({})", sym.name, sym.kind),
                    symbols: vec![sym.name.clone()],
                    inferred: true,
                    confidence: 0.7,
                });
                new_links.push(link);
                link_pairs.insert(pair_key);
            }
        }
    }

    new_links
}

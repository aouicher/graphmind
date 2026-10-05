use graphmind_config::{paths, resolve_project_slug, Registry};
use graphmind_config::config::EmbeddingMode;
use graphmind_embeddings::factory::create_engine;
use graphmind_embeddings::search::semantic_search;
use colored::Colorize;
use graphmind_db::queries::GraphQueries;
use graphmind_db::schema::init_database;
use std::collections::{BTreeMap, HashMap};

pub fn search(query: &str, slug: Option<&str>, limit: i64, kind: Option<&str>, offset: usize, include_content: bool) {
    let slug = match resolve_project_slug(&[slug]) {
        Some(s) => s,
        None => {
            eprintln!(
                "{} No project specified and none could be resolved",
                "Error:".red().bold()
            );
            std::process::exit(1);
        }
    };

    let db_path = paths::graph_db_path(&slug);
    if !db_path.exists() {
        eprintln!(
            "{} No graph found for {}. Run 'graphmind build' first.",
            "Error:".red().bold(),
            slug
        );
        std::process::exit(1);
    }

    let db_path_str = db_path.to_string_lossy().to_string();
    let db = init_database(&db_path_str).unwrap_or_else(|e| {
        eprintln!("{} Failed to open database: {}", "Error:".red().bold(), e);
        std::process::exit(1);
    });

    let q = GraphQueries::new(&db);

    // FTS search
    let fts_query = graphmind_db::queries::fts_prefix_query(query);
    // Hits needed before --offset is applied.
    let wanted = limit as usize + offset;

    let fts_results = if fts_query.is_empty() {
        Vec::new()
    } else {
        let raw = q.search_symbols(&fts_query, wanted as i64 * 5);
        if let Some(k) = kind {
            raw.into_iter().filter(|r| r.kind.eq_ignore_ascii_case(k)).take(wanted).collect()
        } else {
            raw.into_iter().take(wanted).collect()
        }
    };

    // Semantic search (if embeddings configured)
    let config = Registry::get_config();
    let semantic_results = if config.embedding.mode != EmbeddingMode::Disabled {
        let emb_db_path = paths::embedding_db_path(&slug);
        if emb_db_path.exists() {
            if let Ok(engine) = create_engine(&config.embedding) {
                semantic_search(
                    &emb_db_path,
                    query,
                    &|text| engine.embed(text).ok(),
                    wanted,
                    kind,
                )
            } else {
                Vec::new()
            }
        } else {
            Vec::new()
        }
    } else {
        Vec::new()
    };

    // Fuse results with RRF
    // Keep enough fused hits for --offset paging.
    let mut results = fuse_results(&fts_results, &semantic_results, wanted, |file, name| {
        q.symbols_in_file(file).into_iter().find(|s| s.name == name)
    });

    // Graph expansion: boost seeds with 1-hop neighbors
    let graph_boost = 0.3;
    // BTreeMap: neighbors are merged in a fixed order, so equal scores rank the same every run.
    let mut neighbor_scores: BTreeMap<String, FusedHit> = BTreeMap::new();
    for hit in &results {
        let callers = q.callers(&hit.name);
        for c in callers.iter().take(3) {
            let key = format!("{}:{}:{}", c.file, c.name, c.line_start);
            let boost = hit.score * graph_boost;
            neighbor_scores.entry(key)
                .and_modify(|e| e.score += boost)
                .or_insert_with(|| FusedHit {
                    name: c.name.clone(), kind: c.kind.clone(), file: c.file.clone(),
                    line: c.line_start, signature: c.signature.clone(), content: c.content.clone(),
                    score: boost, from_fts: false, from_sem: false, from_graph: true,
                });
        }
        let callees = q.callees(&hit.name);
        for c in callees.iter().take(3) {
            let key = format!("{}:{}:{}", c.file, c.name, c.line_start);
            let boost = hit.score * graph_boost * 0.5;
            neighbor_scores.entry(key)
                .and_modify(|e| e.score += boost)
                .or_insert_with(|| FusedHit {
                    name: c.name.clone(), kind: c.kind.clone(), file: c.file.clone(),
                    line: c.line_start, signature: c.signature.clone(), content: c.content.clone(),
                    score: boost, from_fts: false, from_sem: false, from_graph: true,
                });
        }
    }

    // Merge graph neighbors into results
    for (key, neighbor) in neighbor_scores {
        let existing = results.iter_mut().find(|r| format!("{}:{}:{}", r.file, r.name, r.line) == key);
        if let Some(e) = existing {
            e.score += neighbor.score;
            e.from_graph = true;
        } else {
            results.push(neighbor);
        }
    }

    sort_hits(&mut results);
    // apply offset before truncate
    if offset > 0 {
        results.drain(..offset.min(results.len()));
    }
    results.truncate(limit as usize);

    if results.is_empty() {
        println!("{} No results for: {}", "!".yellow(), query);
        return;
    }

    let source_tag = if !semantic_results.is_empty() { "FTS+semantic+graph" } else { "FTS+graph" };
    println!(
        "{} {} result(s) for \"{}\" [{}]:\n",
        ">>".cyan().bold(),
        results.len().to_string().green(),
        query,
        source_tag.dimmed()
    );

    for hit in &results {
        let score_str = format!("{:.3}", hit.score);
        let source = match (hit.from_fts, hit.from_sem, hit.from_graph) {
            (true, true, true) => "FTS+SEM+G",
            (true, true, false) => "FTS+SEM",
            (true, false, true) => "FTS+G",
            (false, true, true) => "SEM+G",
            (true, false, false) => "FTS",
            (false, true, false) => "SEM",
            (false, false, true) => "GRAPH",
            _ => "?",
        };
        println!(
            "  {} [{}] {}:{} ({}) [{}]",
            hit.name.bold(),
            hit.kind.yellow(),
            hit.file.dimmed(),
            hit.line,
            score_str.dimmed(),
            source.cyan()
        );
        if let Some(ref s) = hit.signature {
            if !s.is_empty() {
                println!("    {}", s.dimmed());
            }
        }
        if include_content {
            if let Some(ref content) = hit.content {
                if !content.is_empty() {
                    println!("{}", content.dimmed());
                }
            }
        }
    }
}

pub fn embed_status(slug: Option<&str>) {
    let slug = match resolve_project_slug(&[slug]) {
        Some(s) => s,
        None => {
            eprintln!("{} No project specified", "Error:".red().bold());
            std::process::exit(1);
        }
    };

    let config = Registry::get_config();
    let mode = &config.embedding.mode;
    println!("{} Embedding mode: {:?}", ">>".cyan().bold(), mode);

    if *mode == EmbeddingMode::Disabled {
        println!("  Configure in ~/.graphmind/config.json to enable");
        return;
    }

    let emb_db_path = paths::embedding_db_path(&slug);
    if !emb_db_path.exists() {
        println!("  {} No embedding index yet. Run 'graphmind build' to generate.", "!".yellow());
        return;
    }

    if let Ok(store) = graphmind_embeddings::store::EmbeddingStore::open(&emb_db_path) {
        let count = store.count().unwrap_or(0);
        let files = store.files_indexed().len();
        let model = store.get_meta("model").unwrap_or_else(|| "unknown".to_string());
        println!("  Model: {}", model.cyan());
        println!("  Symbols: {}", count.to_string().green());
        println!("  Files: {}", files.to_string().green());
    }
}

struct FusedHit {
    name: String,
    kind: String,
    file: String,
    line: i64,
    signature: Option<String>,
    content: Option<String>,
    score: f64,
    from_fts: bool,
    from_sem: bool,
    from_graph: bool,
}

/// Highest score first; ties broken by location so the order never depends on hash order.
fn sort_hits(hits: &mut [FusedHit]) {
    hits.sort_by(|a, b| {
        b.score
            .total_cmp(&a.score)
            .then_with(|| a.file.cmp(&b.file))
            .then_with(|| a.line.cmp(&b.line))
            .then_with(|| a.name.cmp(&b.name))
    });
}

/// Reciprocal-rank fusion of FTS and semantic hits. Embeddings store no line number, so a
/// semantic hit is matched to its symbol by (file, name): it adds to the FTS hit for the same
/// symbol, or is looked up with `resolve` to get its line, signature and body.
fn fuse_results(
    fts: &[graphmind_db::queries::SymbolRow],
    semantic: &[graphmind_embeddings::search::SearchResult],
    limit: usize,
    resolve: impl Fn(&str, &str) -> Option<graphmind_db::queries::SymbolRow>,
) -> Vec<FusedHit> {
    let k = 60.0;
    let mut hits: Vec<FusedHit> = Vec::new();
    let mut by_symbol: HashMap<(String, String), usize> = HashMap::new();
    let mut by_location: HashMap<(String, String, i64), usize> = HashMap::new();

    for (i, s) in fts.iter().enumerate() {
        let rrf = 1.0 / (k + i as f64 + 1.0);
        if let Some(&idx) = by_location.get(&(s.file.clone(), s.name.clone(), s.line_start)) {
            hits[idx].score += rrf;
            continue;
        }
        by_location.insert((s.file.clone(), s.name.clone(), s.line_start), hits.len());
        by_symbol.entry((s.file.clone(), s.name.clone())).or_insert(hits.len());
        hits.push(FusedHit {
            name: s.name.clone(), kind: s.kind.clone(), file: s.file.clone(),
            line: s.line_start, signature: s.signature.clone(), content: s.content.clone(),
            score: rrf, from_fts: true, from_sem: false, from_graph: false,
        });
    }

    for (i, r) in semantic.iter().enumerate() {
        let rrf = 1.0 / (k + i as f64 + 1.0);
        let key = (r.file.clone(), r.symbol_name.clone());
        if let Some(&idx) = by_symbol.get(&key) {
            hits[idx].score += rrf;
            hits[idx].from_sem = true;
            continue;
        }
        by_symbol.insert(key, hits.len());
        let sym = resolve(&r.file, &r.symbol_name);
        hits.push(FusedHit {
            name: r.symbol_name.clone(), kind: r.symbol_kind.clone(), file: r.file.clone(),
            line: sym.as_ref().map_or(0, |s| s.line_start),
            signature: sym.as_ref().and_then(|s| s.signature.clone()),
            content: sym.and_then(|s| s.content),
            score: rrf, from_fts: false, from_sem: true, from_graph: false,
        });
    }

    sort_hits(&mut hits);
    hits.truncate(limit);
    hits
}

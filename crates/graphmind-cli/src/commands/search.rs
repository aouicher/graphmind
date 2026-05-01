use graphmind_config::{paths, resolve_project_slug, Registry};
use graphmind_config::config::EmbeddingMode;
use graphmind_embeddings::factory::create_engine;
use graphmind_embeddings::search::semantic_search;
use colored::Colorize;
use graphmind_db::queries::GraphQueries;
use graphmind_db::schema::init_database;
use std::collections::HashMap;

pub fn search(query: &str, slug: Option<&str>, limit: i64, kind: Option<&str>) {
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
    let fts_query = query
        .split(';')
        .flat_map(|part| part.split_whitespace())
        .filter(|w| w.len() > 1)
        .map(|w| format!("{w}*"))
        .collect::<Vec<_>>()
        .join(" OR ");

    let fts_results = if fts_query.is_empty() {
        Vec::new()
    } else {
        let raw = q.search_symbols(&fts_query, limit * 5);
        if let Some(k) = kind {
            raw.into_iter().filter(|r| r.kind.eq_ignore_ascii_case(k)).take(limit as usize).collect()
        } else {
            raw.into_iter().take(limit as usize).collect()
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
                    limit as usize,
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
    let results = fuse_results(&fts_results, &semantic_results, limit as usize);

    if results.is_empty() {
        println!("{} No results for: {}", "!".yellow(), query);
        return;
    }

    let source_tag = if !semantic_results.is_empty() { "FTS+semantic" } else { "FTS" };
    println!(
        "{} {} result(s) for \"{}\" [{}]:\n",
        ">>".cyan().bold(),
        results.len().to_string().green(),
        query,
        source_tag.dimmed()
    );

    for hit in &results {
        let score_str = format!("{:.3}", hit.score);
        let source = match (hit.from_fts, hit.from_sem) {
            (true, true) => "FTS+SEM",
            (true, false) => "FTS",
            (false, true) => "SEM",
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
    score: f64,
    from_fts: bool,
    from_sem: bool,
}

fn fuse_results(
    fts: &[graphmind_db::queries::SymbolRow],
    semantic: &[graphmind_embeddings::search::SearchResult],
    limit: usize,
) -> Vec<FusedHit> {
    let k = 60.0;
    let mut scores: HashMap<String, FusedHit> = HashMap::new();

    for (i, s) in fts.iter().enumerate() {
        let key = format!("{}:{}:{}", s.file, s.name, s.line_start);
        let rrf = 1.0 / (k + i as f64 + 1.0);
        scores.entry(key)
            .and_modify(|e| { e.score += rrf; e.from_fts = true; })
            .or_insert_with(|| FusedHit {
                name: s.name.clone(), kind: s.kind.clone(), file: s.file.clone(),
                line: s.line_start, signature: s.signature.clone(), score: rrf,
                from_fts: true, from_sem: false,
            });
    }

    for (i, r) in semantic.iter().enumerate() {
        let key = format!("{}:{}:0", r.file, r.symbol_name);
        let rrf = 1.0 / (k + i as f64 + 1.0);
        scores.entry(key)
            .and_modify(|e| { e.score += rrf; e.from_sem = true; })
            .or_insert_with(|| FusedHit {
                name: r.symbol_name.clone(), kind: r.symbol_kind.clone(), file: r.file.clone(),
                line: 0, signature: None, score: rrf,
                from_fts: false, from_sem: true,
            });
    }

    let mut results: Vec<_> = scores.into_values().collect();
    results.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
    results.truncate(limit);
    results
}

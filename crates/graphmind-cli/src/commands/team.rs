use colored::Colorize;
use graphmind_config::config::{load_config, save_config, TeamConfig};
use graphmind_config::{paths, Registry};
use graphmind_license::LicenseManager;
use graphmind_config::config::Feature;
use graphmind_memory::store::MemoryStore;
use rusqlite::{Connection, params};
use sha2::{Digest, Sha256};

use crate::http_client::{
    CrossLinkItem, EdgeItem, GraphSyncPayload, GraphmindClient, ImportItem, ListenerItem,
    MemoryPushItem, SymbolItem,
};

// ── build_sync_payload ───────────────────────────────────────

pub fn build_sync_payload(
    slug: &str,
    since: Option<u64>,
) -> Result<GraphSyncPayload, Box<dyn std::error::Error>> {
    let db_path = paths::graph_db_path(slug);
    if !db_path.exists() {
        return Err(format!("Pas de graph pour {slug}. Lance 'graphmind build' d'abord.").into());
    }

    let db = Connection::open(&db_path)?;

    // Read symbols (optionally filtered by files modified since ts)
    // Pattern: collect into named variable before stmt is dropped (rusqlite lifetime)
    let symbols: Vec<SymbolItem> = if let Some(ts) = since {
        let sql =
            "SELECT s.name, s.kind, s.file, s.line_start, s.line_end, s.content, s.signature
             FROM symbols s
             JOIN files f ON f.path = s.file WHERE f.last_parsed > ?1"
            .to_string();
        let mut stmt = db.prepare(&sql)?;
        let rows = stmt.query_map(params![ts as i64], |row| {
            Ok(SymbolItem {
                name: row.get(0)?,
                kind: row.get(1)?,
                file_path: row.get(2)?,
                line_start: row.get(3)?,
                line_end: row.get(4)?,
                source_code: row.get(5)?,
                signature: row.get(6)?,
                parent_name: None,
                qualified_name: None,
            })
        })?;
        rows.filter_map(|r| r.ok()).collect()
    } else {
        let mut stmt = db.prepare(
            "SELECT name, kind, file, line_start, line_end, content, signature FROM symbols",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok(SymbolItem {
                name: row.get(0)?,
                kind: row.get(1)?,
                file_path: row.get(2)?,
                line_start: row.get(3)?,
                line_end: row.get(4)?,
                source_code: row.get(5)?,
                signature: row.get(6)?,
                parent_name: None,
                qualified_name: None,
            })
        })?;
        rows.filter_map(|r| r.ok()).collect()
    };

    // Read call edges (kind NOT IN imports/listens/emits)
    let call_edges: Vec<EdgeItem> = {
        let mut stmt = db.prepare(
            "SELECT s1.name, s1.file, s2.name, s2.file
             FROM edges e
             JOIN symbols s1 ON s1.id = e.from_id
             JOIN symbols s2 ON s2.id = e.to_id
             WHERE e.kind NOT IN ('imports', 'listens', 'emits')",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok(EdgeItem {
                caller_name: row.get(0)?,
                caller_file: row.get(1)?,
                callee_name: row.get(2)?,
                callee_file: row.get(3)?,
            })
        })?;
        rows.filter_map(|r| r.ok()).collect()
    };

    // Read import edges (kind = 'imports') as ImportItem
    let file_imports: Vec<ImportItem> = {
        let mut stmt = db.prepare(
            "SELECT s1.file, s2.file
             FROM edges e
             JOIN symbols s1 ON s1.id = e.from_id
             JOIN symbols s2 ON s2.id = e.to_id
             WHERE e.kind = 'imports'",
        )?;
        let rows = stmt.query_map([], |row| {
            let source_file: String = row.get(0)?;
            let target_file: String = row.get(1)?;
            Ok(ImportItem {
                source_file,
                source_project: slug.to_string(),
                target_file,
                target_project: slug.to_string(),
                import_type: Some("imports".to_string()),
            })
        })?;
        rows.filter_map(|r| r.ok()).collect()
    };

    // Read event listeners (kind = 'listens')
    let event_listeners: Vec<ListenerItem> = {
        let mut stmt = db.prepare(
            "SELECT s1.name, s2.name, s2.file
             FROM edges e
             JOIN symbols s1 ON s1.id = e.from_id
             JOIN symbols s2 ON s2.id = e.to_id
             WHERE e.kind = 'listens'",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok(ListenerItem {
                event_name: row.get(0)?,
                listener_name: row.get(1)?,
                listener_file: row.get(2)?,
            })
        })?;
        rows.filter_map(|r| r.ok()).collect()
    };

    // Read cross-project links from JSONL store
    let cross_project_links: Vec<CrossLinkItem> = {
        let cross_path = paths::cross_links_path();
        if cross_path.exists() {
            let store = graphmind_memory::cross_links::CrossLinkStore::new(&cross_path);
            store
                .find_by_project(slug)
                .into_iter()
                .map(|l| CrossLinkItem {
                    source_project: l.from.clone(),
                    source_symbol: l.symbols.first().cloned().unwrap_or_default(),
                    target_project: l.to.clone(),
                    target_symbol: l.symbols.get(1).cloned().unwrap_or_default(),
                    link_type: Some(format!("{:?}", l.link_type).to_lowercase()),
                })
                .collect()
        } else {
            Vec::new()
        }
    };

    let since_iso = since.and_then(|ts| {
        chrono::DateTime::from_timestamp(ts as i64, 0).map(|d| d.to_rfc3339())
    });

    Ok(GraphSyncPayload {
        project_slug: slug.to_string(),
        since: since_iso,
        symbols,
        call_edges,
        file_imports,
        cross_project_links,
        event_listeners,
    })
}

// ── graphmind team init ──────────────────────────────────────

pub fn init(team_id: Option<&str>, server_url: Option<&str>) {
    let config = load_config();
    let license = LicenseManager::from_config(&config);

    if !license.has_feature(&Feature::TeamSync) {
        eprintln!(
            "{} Fonctionnalité Team requise.",
            "✗".red().bold()
        );
        eprintln!("→ Passer au plan Team : https://graphmind.app/pricing");
        std::process::exit(1);
    }

    let team_id = match team_id {
        Some(id) => id.to_string(),
        None => {
            eprint!("Team ID : ");
            let mut input = String::new();
            std::io::stdin().read_line(&mut input).ok();
            let trimmed = input.trim().to_string();
            if trimmed.is_empty() {
                eprintln!("{} Team ID requis.", "✗".red().bold());
                std::process::exit(1);
            }
            trimmed
        }
    };

    let server = server_url
        .unwrap_or("https://graphmind-server.fly.dev")
        .trim_end_matches('/')
        .to_string();

    let mut cfg = load_config();
    cfg.team = Some(TeamConfig {
        team_id: team_id.clone(),
        server_url: server.clone(),
        ..TeamConfig::default()
    });
    save_config(&cfg);

    // Test connectivity
    let client = match GraphmindClient::from_config(&cfg) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("{} Impossible de créer le client : {e}", "✗".red().bold());
            std::process::exit(1);
        }
    };

    let projects = Registry::list();
    let connected = if let Some(first) = projects.first() {
        client.graph_status(&first.slug).is_ok()
    } else {
        client.check_connectivity()
    };

    if !connected {
        eprintln!("{} Connexion impossible au serveur.", "✗".red().bold());
        eprintln!("Vérifiez votre clé API et l'URL du serveur.");
        std::process::exit(1);
    }

    println!("{} Team configurée : {}", "✓".green().bold(), team_id.cyan());
    println!("{} Serveur : {}", "✓".green().bold(), server.dimmed());
    println!();
    println!("Prochaines étapes :");
    println!("  graphmind team push    — synchroniser le graph et les memories");
    println!("  graphmind team status  — voir l'état de la sync");
}

// ── graphmind team push ──────────────────────────────────────

pub fn push(
    project_slug: Option<&str>,
    all: bool,
    memories_only: bool,
    graph_only: bool,
) {
    let mut config = load_config();
    let license = LicenseManager::from_config(&config);

    let team_cfg = match config.team.as_ref() {
        Some(t) => t.clone(),
        None => {
            eprintln!("Lance d'abord: graphmind team init");
            std::process::exit(1);
        }
    };

    let client = match GraphmindClient::from_config(&config) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("{} {e}", "✗".red().bold());
            std::process::exit(1);
        }
    };

    if !client.check_connectivity() {
        eprintln!("{} Serveur inaccessible.", "✗".red().bold());
        std::process::exit(1);
    }

    // ── Graph sync ───────────────────────────────────────────
    if !memories_only && license.has_feature(&Feature::TeamSync) {
        let projects: Vec<_> = if let Some(slug) = project_slug {
            Registry::get(slug)
                .map(|p| vec![p])
                .unwrap_or_else(|| {
                    eprintln!("{} Projet '{}' non trouvé.", "✗".red().bold(), slug);
                    std::process::exit(1);
                })
        } else {
            Registry::list()
        };

        for project in &projects {
            let slug = &project.slug;
            let last_sync = config.team.as_ref()
                .and_then(|t| t.last_graph_sync.get(slug))
                .copied();

            let payload = match build_sync_payload(slug, last_sync) {
                Ok(p) => p,
                Err(e) => {
                    eprintln!("{} {} : {e}", "✗".red().bold(), slug);
                    continue;
                }
            };

            if payload.symbols.is_empty() && payload.call_edges.is_empty() {
                println!("  {} : rien de nouveau", slug.dimmed());
                continue;
            }

            match client.sync_graph(&payload) {
                Ok(result) => {
                    let now = now_unix();
                    if let Some(t) = config.team.as_mut() {
                        t.last_graph_sync.insert(slug.clone(), now);
                    }
                    save_config(&config);
                    println!(
                        "{} {}  v{}  {} symboles · {} edges",
                        "↑".cyan().bold(),
                        slug.cyan(),
                        result.version,
                        result.symbols.to_string().green(),
                        result.edges.to_string().green(),
                    );
                }
                Err(e) => {
                    eprintln!("{} {} : {e}", "✗".red().bold(), slug);
                }
            }
        }
    }

    // ── Memory sync ──────────────────────────────────────────
    if !graph_only && license.has_feature(&Feature::TeamMemories) {
        push_memories_impl(&mut config, project_slug);
    } else if !graph_only && !license.has_feature(&Feature::TeamMemories) {
        eprintln!(
            "  {} Memories : plan Team requis pour la sync de memories.",
            "!".yellow()
        );
    }
}

fn push_memories_impl(config: &mut graphmind_config::config::GlobalConfig, project_slug: Option<&str>) {
    let client = match GraphmindClient::from_config(config) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("{} {e}", "✗".red().bold());
            return;
        }
    };

    let memory_dir = paths::memory_dir();
    let store = MemoryStore::new(&memory_dir);

    // Collect all memories to push
    let all_entries = if let Some(slug) = project_slug {
        store.list(Some(slug))
    } else {
        // list all projects + global
        let mut entries = store.list(None);
        for p in Registry::list() {
            entries.extend(store.list(Some(&p.slug)));
        }
        entries.dedup_by_key(|e| e.id.clone());
        entries
    };

    let to_push: Vec<_> = all_entries
        .iter()
        .filter(|e| e.is_shared)
        .filter(|e| e.synced_at.is_none())
        .collect();

    if to_push.is_empty() {
        println!("  Memories : rien de nouveau");
        return;
    }

    let items: Vec<MemoryPushItem> = to_push
        .iter()
        .map(|e| {
            let local_id = sha256_hex(&e.content);
            let project_slug_val = e.project.clone().unwrap_or_else(|| "global".to_string());
            MemoryPushItem {
                local_id,
                project_slug: project_slug_val,
                content: e.content.clone(),
                category: Some(format!("{:?}", e.entry_type).to_lowercase()),
                metadata: None,
                is_deleted: false,
                updated_at: e.updated.clone(),
            }
        })
        .collect();

    match client.push_memories(&items) {
        Ok(result) => {
            // Update synced_at for pushed memories via rewrite
            let now = now_unix();
            let memory_dir = paths::memory_dir();
            let store2 = MemoryStore::new(&memory_dir);
            // Rewrite all JSONL files updating synced_at for pushed entries
            update_memories_synced_at(&store2, &to_push.iter().map(|e| e.id.clone()).collect::<Vec<_>>(), now);

            // Update last_memory_push in config
            if let Some(t) = config.team.as_mut() {
                t.last_memory_push = Some(now);
            }
            save_config(config);

            println!(
                "{} {} memories envoyées",
                "↑".cyan().bold(),
                result.accepted.to_string().green()
            );
        }
        Err(e) => {
            eprintln!("{} Memories push : {e}", "✗".red().bold());
        }
    }
}

// ── graphmind team pull ──────────────────────────────────────

pub fn pull(project_slug: Option<&str>) {
    let mut config = load_config();
    let license = LicenseManager::from_config(&config);

    if config.team.is_none() {
        eprintln!("Lance d'abord: graphmind team init");
        std::process::exit(1);
    }

    if !license.has_feature(&Feature::TeamMemories) {
        eprintln!(
            "{} Plan Team requis pour la sync de memories.",
            "✗".red().bold()
        );
        eprintln!("→ https://graphmind.app/pricing");
        std::process::exit(1);
    }

    let client = match GraphmindClient::from_config(&config) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("{} {e}", "✗".red().bold());
            std::process::exit(1);
        }
    };

    let last_pull = config.team.as_ref().and_then(|t| t.last_memory_pull);

    let result = match client.pull_memories(last_pull, project_slug) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("{} Pull memories : {e}", "✗".red().bold());
            std::process::exit(1);
        }
    };

    if result.memories.is_empty() {
        println!("Memories à jour.");
        return;
    }

    let memory_dir = paths::memory_dir();
    let store = MemoryStore::new(&memory_dir);
    let now = now_unix();
    let now_iso = chrono::Utc::now().to_rfc3339();

    let mut count_new = 0usize;
    let mut count_updated = 0usize;
    let mut count_deleted = 0usize;

    for item in &result.memories {
        let slug = if item.project_slug == "global" {
            None
        } else {
            Some(item.project_slug.as_str())
        };

        if item.is_deleted {
            // Delete by remote_id
            delete_memory_by_remote_id(&store, &item.id, slug);
            count_deleted += 1;
            continue;
        }

        // Check if remote_id already exists locally
        let existing = find_memory_by_remote_id(&store, &item.id, slug);

        match existing {
            None => {
                // New memory — insert
                use graphmind_memory::store::{AddOptions, MemoryType};
                let entry_type = MemoryType::Context;
                let mut opts = AddOptions {
                    project: slug.map(|s| s.to_string()),
                    global: slug.is_none(),
                    entry_type,
                    tags: Vec::new(),
                    priority: false,
                };
                let mut new_entry = store.add(&item.content, opts);
                // We need to update the entry with remote_id and synced_at
                // Since MemoryStore.add() doesn't support these fields yet on write path,
                // we patch via rewrite
                update_memory_remote_fields(
                    &store,
                    &new_entry.id,
                    &item.id,
                    now,
                    slug,
                );
                count_new += 1;
            }
            Some(existing_entry) => {
                // Compare updated_at
                let remote_updated = parse_iso_to_unix(&item.updated_at).unwrap_or(0);
                let local_synced = existing_entry.synced_at.unwrap_or(0);
                if remote_updated > local_synced {
                    update_memory_content(
                        &store,
                        &existing_entry.id,
                        &item.content,
                        &item.category,
                        now,
                        slug,
                    );
                    count_updated += 1;
                }
                // else: local is newer, skip
            }
        }
    }

    // Update last_memory_pull
    if let Some(t) = config.team.as_mut() {
        t.last_memory_pull = Some(now);
    }
    save_config(&config);

    if count_new == 0 && count_updated == 0 && count_deleted == 0 {
        println!("Memories à jour.");
    } else {
        println!(
            "{} {} nouvelles · {} mises à jour · {} supprimées",
            "↓".cyan().bold(),
            count_new.to_string().green(),
            count_updated.to_string().yellow(),
            count_deleted.to_string().red(),
        );
    }
}

// ── graphmind team status ────────────────────────────────────

pub fn status() {
    let config = load_config();
    let license = LicenseManager::from_config(&config);

    if !license.has_feature(&Feature::TeamSync) {
        eprintln!(
            "{} Plan Team requis.",
            "✗".red().bold()
        );
        eprintln!("→ https://graphmind.app/pricing");
        std::process::exit(1);
    }

    let team_cfg = match config.team.as_ref() {
        Some(t) => t,
        None => {
            eprintln!("Team non configurée. Lance: graphmind team init");
            std::process::exit(1);
        }
    };

    println!("Team   : {}", team_cfg.team_id.cyan());
    println!("Serveur: {}", team_cfg.server_url.dimmed());

    let client = match GraphmindClient::from_config(&config) {
        Ok(c) => c,
        Err(e) => {
            println!();
            println!("{} Serveur inaccessible — affichage local uniquement", "⚠".yellow().bold());
            show_local_status(&config);
            return;
        }
    };

    if !client.check_connectivity() {
        println!();
        println!("{} Serveur inaccessible — affichage local uniquement", "⚠".yellow().bold());
        show_local_status(&config);
        return;
    }

    // Graph status per project
    let projects = Registry::list();
    if !projects.is_empty() {
        println!();
        println!("Graph :");
        for p in &projects {
            let last_sync = team_cfg.last_graph_sync.get(&p.slug).copied();
            let sync_label = match last_sync {
                None => "jamais synchronisé".yellow().to_string(),
                Some(ts) => format!("sync {}", relative_time(ts)).dimmed().to_string(),
            };
            match client.graph_status(&p.slug) {
                Ok(gs) if gs.exists => {
                    println!(
                        "  {:<25} v{}  ({})",
                        p.slug.cyan(),
                        gs.version.unwrap_or(0),
                        sync_label
                    );
                }
                _ => {
                    println!(
                        "  {:<25} {}",
                        p.slug.cyan(),
                        "jamais synchronisé".yellow()
                    );
                }
            }
        }
    }

    // Memory status
    let memory_dir = paths::memory_dir();
    let store = MemoryStore::new(&memory_dir);
    let pending_push = count_memories_pending_push(&store);

    let remote_count = client.memory_count().unwrap_or(0);

    println!();
    println!("Memories :");
    if pending_push > 0 {
        println!(
            "  {} en attente de push",
            pending_push.to_string().yellow()
        );
    }
    if remote_count > 0 {
        println!(
            "  {} nouvelles sur le serveur",
            remote_count.to_string().cyan()
        );
    }
    if pending_push == 0 && remote_count == 0 {
        println!("  À jour.");
    }

    if pending_push > 0 {
        println!();
        println!("→ {} pour envoyer", "graphmind team push".bold());
    }
    if remote_count > 0 {
        println!("→ {} pour recevoir", "graphmind team pull".bold());
    }
}

fn show_local_status(config: &graphmind_config::config::GlobalConfig) {
    let memory_dir = paths::memory_dir();
    let store = MemoryStore::new(&memory_dir);
    let pending = count_memories_pending_push(&store);
    println!("  Memories locales non pushées : {}", pending.to_string().yellow());
}

// ── auto_push_graph (appelé depuis build.rs) ─────────────────

pub fn auto_push_graph(slug: &str) -> Result<(), Box<dyn std::error::Error>> {
    let config = load_config();

    let client = GraphmindClient::from_config(&config)?;

    let team_cfg = config.team.as_ref().ok_or("NoTeamConfig")?;
    let last_sync = team_cfg.last_graph_sync.get(slug).copied();

    let payload = build_sync_payload(slug, last_sync)?;

    if payload.symbols.is_empty() && payload.call_edges.is_empty() {
        return Ok(());
    }

    let result = client.sync_graph(&payload)?;

    // Update config
    let mut cfg = load_config();
    if let Some(t) = cfg.team.as_mut() {
        t.last_graph_sync.insert(slug.to_string(), now_unix());
    }
    save_config(&cfg);

    Ok(())
}

// ── Helpers ──────────────────────────────────────────────────

fn now_unix() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

fn sha256_hex(s: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(s.as_bytes());
    hex::encode(hasher.finalize())
}

fn parse_iso_to_unix(s: &str) -> Option<u64> {
    chrono::DateTime::parse_from_rfc3339(s)
        .ok()
        .map(|dt| dt.timestamp() as u64)
}

fn relative_time(ts: u64) -> String {
    let now = now_unix();
    let diff = now.saturating_sub(ts);
    if diff < 60 {
        "à l'instant".to_string()
    } else if diff < 3600 {
        format!("il y a {}min", diff / 60)
    } else if diff < 86400 {
        format!("il y a {}h", diff / 3600)
    } else {
        format!("il y a {}j", diff / 86400)
    }
}

fn count_memories_pending_push(store: &MemoryStore) -> usize {
    let mut all = store.list(None);
    for p in Registry::list() {
        all.extend(store.list(Some(&p.slug)));
    }
    all.dedup_by_key(|e| e.id.clone());
    all.iter()
        .filter(|e| e.is_shared && e.synced_at.is_none())
        .count()
}

fn update_memories_synced_at(store: &MemoryStore, ids: &[String], now: u64) {
    // Rewrite all JSONL files updating synced_at for matching IDs
    let paths_to_update: Vec<std::path::PathBuf> = {
        let memory_dir = paths::memory_dir();
        let mut ps = Vec::new();
        // global
        let g = memory_dir.join("global.jsonl");
        if g.exists() { ps.push(g); }
        // per-project
        for p in Registry::list() {
            let pp = memory_dir.join(format!("{}.jsonl", p.slug));
            if pp.exists() { ps.push(pp); }
        }
        ps
    };

    for file_path in paths_to_update {
        rewrite_jsonl_update_synced_at(&file_path, ids, now);
    }
}

fn rewrite_jsonl_update_synced_at(file_path: &std::path::Path, ids: &[String], now: u64) {
    let content = match std::fs::read_to_string(file_path) {
        Ok(c) => c,
        Err(_) => return,
    };
    let updated: String = content
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| {
            let mut entry: serde_json::Value = match serde_json::from_str(l) {
                Ok(v) => v,
                Err(_) => return l.to_string(),
            };
            if let Some(id) = entry.get("id").and_then(|v| v.as_str()) {
                if ids.contains(&id.to_string()) {
                    entry["synced_at"] = serde_json::json!(now);
                }
            }
            serde_json::to_string(&entry).unwrap_or_else(|_| l.to_string())
        })
        .collect::<Vec<_>>()
        .join("\n")
        + "\n";

    let tmp = file_path.with_extension(format!("tmp.{}", now));
    std::fs::write(&tmp, updated).ok();
    std::fs::rename(&tmp, file_path).ok();
}

fn find_memory_by_remote_id(
    store: &MemoryStore,
    remote_id: &str,
    project: Option<&str>,
) -> Option<graphmind_memory::store::MemoryEntry> {
    store
        .list(project)
        .into_iter()
        .find(|e| e.remote_id.as_deref() == Some(remote_id))
}

fn delete_memory_by_remote_id(store: &MemoryStore, remote_id: &str, project: Option<&str>) {
    if let Some(entry) = find_memory_by_remote_id(store, remote_id, project) {
        store.delete(&entry.id, project);
    }
}

fn update_memory_remote_fields(
    store: &MemoryStore,
    entry_id: &str,
    remote_id: &str,
    now: u64,
    project: Option<&str>,
) {
    let memory_dir = paths::memory_dir();
    let file_path = if let Some(slug) = project {
        memory_dir.join(format!("{slug}.jsonl"))
    } else {
        memory_dir.join("global.jsonl")
    };
    if !file_path.exists() {
        return;
    }
    rewrite_jsonl_set_remote_fields(&file_path, entry_id, remote_id, now);
}

fn rewrite_jsonl_set_remote_fields(
    file_path: &std::path::Path,
    entry_id: &str,
    remote_id: &str,
    now: u64,
) {
    let content = match std::fs::read_to_string(file_path) {
        Ok(c) => c,
        Err(_) => return,
    };
    let updated: String = content
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| {
            let mut entry: serde_json::Value = match serde_json::from_str(l) {
                Ok(v) => v,
                Err(_) => return l.to_string(),
            };
            if entry.get("id").and_then(|v| v.as_str()) == Some(entry_id) {
                entry["remote_id"] = serde_json::json!(remote_id);
                entry["synced_at"] = serde_json::json!(now);
            }
            serde_json::to_string(&entry).unwrap_or_else(|_| l.to_string())
        })
        .collect::<Vec<_>>()
        .join("\n")
        + "\n";

    let tmp = file_path.with_extension(format!("tmp.{now}"));
    std::fs::write(&tmp, updated).ok();
    std::fs::rename(&tmp, file_path).ok();
}

fn update_memory_content(
    store: &MemoryStore,
    entry_id: &str,
    content: &str,
    category: &Option<String>,
    now: u64,
    project: Option<&str>,
) {
    let memory_dir = paths::memory_dir();
    let file_path = if let Some(slug) = project {
        memory_dir.join(format!("{slug}.jsonl"))
    } else {
        memory_dir.join("global.jsonl")
    };
    if !file_path.exists() {
        return;
    }
    let file_content = match std::fs::read_to_string(&file_path) {
        Ok(c) => c,
        Err(_) => return,
    };
    let updated: String = file_content
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| {
            let mut entry: serde_json::Value = match serde_json::from_str(l) {
                Ok(v) => v,
                Err(_) => return l.to_string(),
            };
            if entry.get("id").and_then(|v| v.as_str()) == Some(entry_id) {
                entry["content"] = serde_json::json!(content);
                entry["synced_at"] = serde_json::json!(now);
                entry["updated"] = serde_json::json!(
                    chrono::DateTime::from_timestamp(now as i64, 0)
                        .map(|d| d.to_rfc3339())
                        .unwrap_or_default()
                );
            }
            serde_json::to_string(&entry).unwrap_or_else(|_| l.to_string())
        })
        .collect::<Vec<_>>()
        .join("\n")
        + "\n";

    let tmp = file_path.with_extension(format!("tmp.{now}"));
    std::fs::write(&tmp, updated).ok();
    std::fs::rename(&tmp, file_path).ok();
}

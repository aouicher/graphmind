use crate::cache::FileHashCache;
use crate::markdown::parse_markdown;
use crate::schema::init_database;
use graphmind_core::{parse_single, ParsedFile};
use rusqlite::{params, Connection};
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

const LANGUAGE_MAP: &[(&str, &str)] = &[
    (".ts", "typescript"),
    (".tsx", "typescript"),
    (".js", "javascript"),
    (".jsx", "javascript"),
    (".mjs", "javascript"),
    (".py", "python"),
    (".go", "go"),
    (".rs", "rust"),
    (".rb", "ruby"),
    (".tf", "hcl"),
    (".tfvars", "hcl"),
    (".yml", "yaml"),
    (".yaml", "yaml"),
    (".md", "markdown"),
];

const DEFAULT_EXCLUDE: &[&str] = &[
    "node_modules",
    "dist",
    "build",
    "out",
    ".git",
    ".next",
    ".nuxt",
    ".turbo",
    "coverage",
    "__pycache__",
    ".venv",
    "venv",
    "vendor",
    "target",
    "tmp",
    "log",
    "cdk.out",
    "public/packs",
    "public/assets",
    ".terraform",
];

fn extension_to_language(ext: &str) -> Option<&'static str> {
    LANGUAGE_MAP.iter().find(|(e, _)| *e == ext).map(|(_, l)| *l)
}

#[derive(Debug, Clone)]
pub struct BuildResult {
    pub files_processed: usize,
    pub symbols_found: usize,
    pub edges_created: usize,
    pub skipped: usize,
    pub deleted: usize,
    pub duration_ms: u128,
}

pub struct BuildOptions {
    pub full: bool,
    pub exclude: Vec<String>,
}

impl Default for BuildOptions {
    fn default() -> Self {
        Self {
            full: false,
            exclude: DEFAULT_EXCLUDE.iter().map(|s| s.to_string()).collect(),
        }
    }
}

struct SourceFile {
    full_path: PathBuf,
    language: String,
}

struct ParsedImport {
    source: String,
    specifiers: Vec<String>,
}

struct ParsedCallSite {
    caller: String,
    callee: String,
}

pub struct GraphBuilder {
    db: Connection,
    cache: FileHashCache,
}

impl GraphBuilder {
    pub fn new(db_path: &str, cache_dir: &str) -> Self {
        if let Some(parent) = Path::new(db_path).parent() {
            fs::create_dir_all(parent).ok();
        }
        let db = init_database(db_path).expect("Failed to init database");
        let cache = FileHashCache::new(Path::new(cache_dir));
        Self { db, cache }
    }

    pub fn build(&mut self, project_path: &str, options: &BuildOptions) -> BuildResult {
        let start = std::time::Instant::now();
        let files = self.collect_files(project_path, &options.exclude);

        let mut processed = 0usize;
        let mut skipped = 0usize;
        let mut deleted = 0usize;
        let mut total_symbols = 0usize;
        let mut total_edges = 0usize;

        // Detect and clean up deleted files
        if !options.full {
            let current_paths: HashSet<String> = files.iter()
                .map(|f| pathdiff(f.full_path.to_str().unwrap_or(""), project_path))
                .collect();
            let cached_paths = self.cache.known_files();
            for stale in &cached_paths {
                if !current_paths.contains(stale) {
                    self.db.execute(
                        "DELETE FROM edges WHERE from_id IN (SELECT id FROM symbols WHERE file = ?1) OR to_id IN (SELECT id FROM symbols WHERE file = ?1)",
                        params![stale],
                    ).ok();
                    self.db.execute("DELETE FROM symbols WHERE file = ?1", params![stale]).ok();
                    self.db.execute("DELETE FROM files WHERE path = ?1", params![stale]).ok();
                    self.cache.remove(stale);
                    deleted += 1;
                }
            }
            if deleted > 0 {
                eprintln!("Cleaned up {} deleted files", deleted);
            }
        }

        eprintln!("Found {} source files", files.len());

        // Pass 1: Parse all files, insert symbols
        let mut pending_imports: Vec<(String, Vec<ParsedImport>)> = Vec::new();
        let mut pending_calls: Vec<(String, Vec<ParsedCallSite>)> = Vec::new();

        let log_interval = (files.len() / 10).max(1);

        for (idx, file) in files.iter().enumerate() {
            if (idx + 1) % log_interval == 0 || idx + 1 == files.len() {
                eprintln!("Parsing {}/{}...", idx + 1, files.len());
            }

            let content = match fs::read_to_string(&file.full_path) {
                Ok(c) => c,
                Err(_) => continue,
            };

            let rel_path = pathdiff(file.full_path.to_str().unwrap_or(""), project_path);

            if !options.full && !self.cache.has_changed(&rel_path, &content) {
                skipped += 1;
                continue;
            }

            if let Err(e) = self.db.execute(
                "DELETE FROM edges WHERE from_id IN (SELECT id FROM symbols WHERE file = ?1) OR to_id IN (SELECT id FROM symbols WHERE file = ?1)",
                params![rel_path],
            ) {
                eprintln!("warn: failed to clean edges for {}: {}", rel_path, e);
            }
            if let Err(e) = self.db.execute("DELETE FROM symbols WHERE file = ?1", params![rel_path]) {
                eprintln!("warn: failed to clean symbols for {}: {}", rel_path, e);
            }

            let parsed = if file.language == "markdown" {
                let md = parse_markdown(file.full_path.to_str().unwrap_or(""), &content);
                ParsedFile {
                    path: file.full_path.to_str().unwrap_or("").to_string(),
                    language: "markdown".to_string(),
                    symbols: md.symbols.iter().map(|s| graphmind_core::Symbol {
                        name: s.name.clone(),
                        kind: match s.kind.as_str() {
                            "class" => graphmind_core::SymbolKind::Class,
                            "function" => graphmind_core::SymbolKind::Function,
                            _ => graphmind_core::SymbolKind::Type,
                        },
                        line_start: s.line_start as u32,
                        line_end: s.line_end as u32,
                        signature: s.signature.clone(),
                        doc: s.doc.clone(),
                    }).collect(),
                    call_sites: Vec::new(),
                    imports: md.imports.iter().map(|i| graphmind_core::ResolvedImport {
                        source: i.source.clone(),
                        specifiers: i.specifiers.clone(),
                        line: i.line as u32,
                        is_default: i.is_default,
                        from_file: i.from_file.clone(),
                    }).collect(),
                }
            } else {
                match parse_single(file.full_path.to_str().unwrap_or(""), &content, &file.language) {
                    Ok(p) => p,
                    Err(_) => continue,
                }
            };

            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs() as i64;

            if let Err(e) = self.db.execute(
                "INSERT OR REPLACE INTO files (path, language, hash, last_parsed) VALUES (?1, ?2, ?3, ?4)",
                params![rel_path, file.language, "", now],
            ) {
                eprintln!("warn: failed to insert file {}: {}", rel_path, e);
            }

            let lines: Vec<&str> = content.split('\n').collect();
            for sym in &parsed.symbols {
                let start_idx = (sym.line_start as usize).saturating_sub(1);
                let end_idx = sym.line_end as usize;
                let body: String = lines[start_idx..end_idx.min(lines.len())].join("\n");

                let kind_str = match sym.kind {
                    graphmind_core::SymbolKind::Function => "Function",
                    graphmind_core::SymbolKind::Class => "Class",
                    graphmind_core::SymbolKind::Method => "Method",
                    graphmind_core::SymbolKind::Interface => "Interface",
                    graphmind_core::SymbolKind::Type => "Type",
                };

                if let Err(e) = self.db.execute(
                    "INSERT INTO symbols (name, kind, file, line_start, line_end, signature, doc, content) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                    params![sym.name, kind_str, rel_path, sym.line_start, sym.line_end, sym.signature, sym.doc, body],
                ) {
                    eprintln!("warn: failed to insert symbol {} in {}: {}", sym.name, rel_path, e);
                    continue;
                }
                total_symbols += 1;
            }

            if !parsed.call_sites.is_empty() {
                pending_calls.push((
                    rel_path.clone(),
                    parsed.call_sites.iter().map(|cs| ParsedCallSite {
                        caller: cs.caller.clone(),
                        callee: cs.callee.clone(),
                    }).collect(),
                ));
            }

            if !parsed.imports.is_empty() {
                pending_imports.push((
                    rel_path.clone(),
                    parsed.imports.iter().map(|imp| ParsedImport {
                        source: imp.source.clone(),
                        specifiers: imp.specifiers.clone(),
                    }).collect(),
                ));
            }

            processed += 1;
            self.cache.update(&rel_path, &content);
        }

        // Pass 2: Resolve cross-file edges
        if let Err(e) = self.db.execute_batch("BEGIN TRANSACTION") {
            eprintln!("warn: failed to begin transaction: {}", e);
        }

        // Build import lookup: file -> (callee_name -> resolved_file)
        let mut import_map: std::collections::HashMap<String, std::collections::HashMap<String, String>> = std::collections::HashMap::new();
        for (rel_path, imports) in &pending_imports {
            let mut file_imports = std::collections::HashMap::new();
            for imp in imports {
                if let Some(resolved) = self.resolve_import_path(&imp.source, rel_path) {
                    for spec in &imp.specifiers {
                        let clean = spec.trim_start_matches("* as ").to_string();
                        file_imports.insert(clean, resolved.clone());
                    }
                }
            }
            if !file_imports.is_empty() {
                import_map.insert(rel_path.clone(), file_imports);
            }
        }

        for (rel_path, call_sites) in &pending_calls {
            for cs in call_sites {
                let caller_id: Option<i64> = self.db.query_row(
                    "SELECT id FROM symbols WHERE name = ?1 AND file = ?2",
                    params![cs.caller, rel_path],
                    |r| r.get(0),
                ).ok();

                let Some(caller_id) = caller_id else { continue };

                // 1. Same file
                let callee_id: Option<i64> = self.db.query_row(
                    "SELECT id FROM symbols WHERE name = ?1 AND file = ?2",
                    params![cs.callee, rel_path],
                    |r| r.get(0),
                ).ok();

                // 2. Import-resolved file
                let callee_id = callee_id.or_else(|| {
                    import_map.get(rel_path.as_str())
                        .and_then(|m| m.get(&cs.callee))
                        .and_then(|target_file| {
                            self.db.query_row(
                                "SELECT id FROM symbols WHERE name = ?1 AND file = ?2",
                                params![cs.callee, target_file],
                                |r| r.get(0),
                            ).ok()
                        })
                });

                // 3. Global fallback
                let (callee_id, confidence) = if let Some(id) = callee_id {
                    (Some(id), 1.0f64)
                } else {
                    let global_id: Option<i64> = self.db.query_row(
                        "SELECT id FROM symbols WHERE name = ?1",
                        params![cs.callee],
                        |r| r.get(0),
                    ).ok();
                    (global_id, 0.5f64)
                };

                if let Some(callee_id) = callee_id {
                    if let Err(e) = self.db.execute(
                        "INSERT INTO edges (from_id, to_id, kind, file, confidence) VALUES (?1, ?2, ?3, ?4, ?5)",
                        params![caller_id, callee_id, "calls", rel_path, confidence],
                    ) {
                        eprintln!("warn: failed to insert call edge in {}: {}", rel_path, e);
                    } else {
                        total_edges += 1;
                    }
                }
            }
        }

        for (rel_path, imports) in &pending_imports {
            for imp in imports {
                let resolved_file = self.resolve_import_path(&imp.source, rel_path);

                for spec in &imp.specifiers {
                    let clean_spec = spec.trim_start_matches("* as ");

                    let target_id: Option<i64> = if let Some(ref rf) = resolved_file {
                        self.db.query_row(
                            "SELECT id FROM symbols WHERE name = ?1 AND file = ?2",
                            params![clean_spec, rf],
                            |r| r.get(0),
                        ).ok()
                    } else {
                        None
                    }.or_else(|| {
                        self.db.query_row(
                            "SELECT id FROM symbols WHERE name = ?1",
                            params![clean_spec],
                            |r| r.get(0),
                        ).ok()
                    });

                    let Some(target_id) = target_id else { continue };

                    let source_id: Option<i64> = self.db.query_row(
                        "SELECT id FROM symbols WHERE file = ?1 LIMIT 1",
                        params![rel_path],
                        |r| r.get(0),
                    ).ok();

                    if let Some(source_id) = source_id {
                        if let Err(e) = self.db.execute(
                            "INSERT INTO edges (from_id, to_id, kind, file) VALUES (?1, ?2, ?3, ?4)",
                            params![source_id, target_id, "imports", rel_path],
                        ) {
                            eprintln!("warn: failed to insert import edge in {}: {}", rel_path, e);
                        } else {
                            total_edges += 1;
                        }
                    }
                }
            }
        }

        if let Err(e) = self.db.execute_batch("COMMIT") {
            eprintln!("warn: failed to commit transaction: {}", e);
        }
        self.cache.save();

        BuildResult {
            files_processed: processed,
            symbols_found: total_symbols,
            edges_created: total_edges,
            skipped,
            deleted,
            duration_ms: start.elapsed().as_millis(),
        }
    }

    pub fn database(&self) -> &Connection {
        &self.db
    }

    fn collect_files(&self, project_path: &str, exclude: &[String]) -> Vec<SourceFile> {
        let mut files = Vec::new();
        let gitignore_patterns = parse_gitignore(project_path);
        let all_excludes: HashSet<String> = exclude.iter().cloned()
            .chain(gitignore_patterns)
            .collect();

        let mut dir_excludes = Vec::new();
        let mut path_excludes = Vec::new();
        let mut file_patterns = Vec::new();

        for e in &all_excludes {
            let clean = e
                .trim_start_matches("**/")
                .trim_end_matches("/**")
                .trim_end_matches("/*")
                .trim_end_matches('/');
            if clean.starts_with("*.") || clean.starts_with('.') {
                file_patterns.push(clean.to_string());
            } else if clean.contains('/') {
                path_excludes.push(clean.to_string());
            } else {
                dir_excludes.push(clean.to_string());
            }
        }

        walk_dir(
            Path::new(project_path),
            Path::new(project_path),
            &dir_excludes,
            &path_excludes,
            &file_patterns,
            &mut files,
        );

        files
    }

    fn resolve_import_path(&self, import_source: &str, from_file: &str) -> Option<String> {
        if !import_source.starts_with('.') {
            return None;
        }

        let dir = Path::new(from_file).parent().unwrap_or(Path::new(""));
        let base = dir.join(import_source).to_string_lossy().replace('\\', "/");

        let extensions = ["", ".ts", ".tsx", ".js", ".jsx", ".mjs", "/index.ts", "/index.js"];
        for ext in &extensions {
            let candidate = format!("{base}{ext}");
            let found: bool = self.db.query_row(
                "SELECT COUNT(*) FROM files WHERE path = ?1",
                params![candidate],
                |r| r.get::<_, i64>(0),
            ).unwrap_or(0) > 0;
            if found {
                return Some(candidate);
            }
        }
        None
    }
}

fn walk_dir(
    dir: &Path,
    root_dir: &Path,
    dir_excludes: &[String],
    path_excludes: &[String],
    file_patterns: &[String],
    files: &mut Vec<SourceFile>,
) {
    let entries = match fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return,
    };

    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();

        if dir_excludes.contains(&name) || name.starts_with('.') {
            continue;
        }

        let full_path = entry.path();
        let rel_path = full_path.strip_prefix(root_dir).unwrap_or(&full_path);
        let rel_str = rel_path.to_string_lossy().to_string();

        if let Ok(ft) = entry.file_type() {
            if ft.is_dir() {
                if path_excludes.iter().any(|p| rel_str == *p || rel_str.starts_with(&format!("{p}/"))) {
                    continue;
                }
                walk_dir(&full_path, root_dir, dir_excludes, path_excludes, file_patterns, files);
            } else {
                if file_patterns.iter().any(|p| name.ends_with(p.trim_start_matches('*'))) {
                    continue;
                }
                if path_excludes.contains(&rel_str) {
                    continue;
                }
                let ext = Path::new(&name).extension()
                    .map(|e| format!(".{}", e.to_string_lossy()))
                    .unwrap_or_default();
                if let Some(language) = extension_to_language(&ext) {
                    files.push(SourceFile {
                        full_path,
                        language: language.to_string(),
                    });
                }
            }
        }
    }
}

fn parse_gitignore(project_path: &str) -> Vec<String> {
    let gitignore_path = Path::new(project_path).join(".gitignore");
    match fs::read_to_string(gitignore_path) {
        Ok(content) => content
            .lines()
            .map(|l| l.trim().to_string())
            .filter(|l| !l.is_empty() && !l.starts_with('#') && !l.starts_with('!'))
            .collect(),
        Err(_) => Vec::new(),
    }
}

fn pathdiff(full_path: &str, base: &str) -> String {
    let full = Path::new(full_path);
    let base = Path::new(base);
    full.strip_prefix(base)
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|_| full_path.to_string())
}

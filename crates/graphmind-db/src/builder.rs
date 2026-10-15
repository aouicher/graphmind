use crate::cache::FileHashCache;
use crate::markdown::parse_markdown;
use crate::paths::project_file;
use crate::schema::init_database;
use graphmind_core::{parse_single, ParsedFile};
use rusqlite::{params, Connection};
use std::collections::{HashMap, HashSet, VecDeque};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

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
    (".c", "c"),
    (".h", "c"),
    (".m", "objc"),
    (".mm", "objc"),
    (".java", "java"),
    (".php", "php"),
    (".swift", "swift"),
    (".sh", "bash"),
    (".bash", "bash"),
    (".zsh", "bash"),
    (".pl", "perl"),
    (".pm", "perl"),
    (".html", "html"),
    (".htm", "html"),
    (".toml", "toml"),
    (".sql", "sql"),
    ("Dockerfile", "dockerfile"),
    (".cpp", "cpp"),
    (".cc", "cpp"),
    (".cxx", "cpp"),
    (".hpp", "cpp"),
    (".hh", "cpp"),
    (".cs", "csharp"),
    (".kt", "kotlin"),
    (".kts", "kotlin"),
    (".dart", "dart"),
    (".scala", "scala"),
    (".sc", "scala"),
    (".r", "r"),
    (".R", "r"),
    (".graphql", "graphql"),
    (".gql", "graphql"),
    (".ps1", "powershell"),
    (".psm1", "powershell"),
];

/// Languages of docs and data files. The builder indexes their headings and keys as symbols;
/// code-only searches leave them out.
pub const DOC_LANGUAGES: &[&str] = &["markdown", "yaml", "toml", "html"];

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
    "env",
    "vendor",
    "target",
    "tmp",
    "log",
    "cdk.out",
    "deployment_packages",
    "public/packs",
    "public/assets",
    ".terraform",
    ".serverless",
    "*.min.js",
    "*.min.css",
    "*.bundle.js",
    "*.chunk.js",
];

fn extension_to_language(ext: &str) -> Option<&'static str> {
    LANGUAGE_MAP.iter().find(|(e, _)| *e == ext).map(|(_, l)| *l)
}

#[derive(Debug, Clone)]
#[derive(Default)]
pub struct BuildResult {
    pub files_processed: usize,
    pub symbols_found: usize,
    pub edges_created: usize,
    pub skipped: usize,
    pub deleted: usize,
    pub duration_ms: u128,
    pub errors: Vec<String>,
}

pub struct BuildOptions {
    pub full: bool,
    pub reset: bool,
    pub exclude: Vec<String>,
    pub cancel: Option<Arc<AtomicBool>>,
    /// If Some, only rebuild these files (relative paths). Bypasses hash cache for them.
    pub only_files: Option<Vec<String>>,
}

impl BuildOptions {
    /// True once the caller asked the build to stop.
    pub fn is_cancelled(&self) -> bool {
        self.cancel.as_ref().is_some_and(|c| c.load(Ordering::Relaxed))
    }
}

impl Default for BuildOptions {
    fn default() -> Self {
        Self {
            full: false,
            reset: false,
            exclude: DEFAULT_EXCLUDE.iter().map(|s| s.to_string()).collect(),
            cancel: None,
            only_files: None,
        }
    }
}

struct SourceFile {
    /// The walked path, the one the graph records.
    full_path: PathBuf,
    /// The path read for content: `full_path`, or for a symlink its resolved target.
    read_path: PathBuf,
    language: String,
}

struct ParsedImport {
    source: String,
    specifiers: Vec<String>,
}

struct ParsedCallSite {
    caller: String,
    callee: String,
    receiver: Option<String>,
    kind: String,
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

    /// Ids of the symbols currently stored for `file`, grouped by (name, kind) in source order.
    fn existing_symbol_ids(&self, file: &str) -> HashMap<(String, String), VecDeque<i64>> {
        let mut map: HashMap<(String, String), VecDeque<i64>> = HashMap::new();
        let rows = self
            .db
            .prepare("SELECT id, name, kind FROM symbols WHERE file = ?1 ORDER BY line_start, id")
            .and_then(|mut stmt| {
                stmt.query_map(params![file], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?)))
                    .map(|rows| rows.filter_map(|r| r.ok()).collect::<Vec<_>>())
            })
            .unwrap_or_default();
        for (id, name, kind) in rows {
            map.entry((name, kind)).or_default().push_back(id);
        }
        map
    }

    /// Whether an earlier build stored anything for `file`: a file row or a hash-cache entry.
    fn is_indexed(&self, file: &str) -> bool {
        self.cache.contains(file)
            || self
                .db
                .query_row("SELECT 1 FROM files WHERE path = ?1", params![file], |_| Ok(()))
                .is_ok()
    }

    /// Removes everything stored for `file`: its edges (both ways), symbols, file row and
    /// hash-cache entry. Failures are printed and added to `errors`.
    fn forget_file(&mut self, file: &str, errors: &mut Vec<String>) {
        if let Err(e) = self.db.execute(
            "DELETE FROM edges WHERE from_id IN (SELECT id FROM symbols WHERE file = ?1) OR to_id IN (SELECT id FROM symbols WHERE file = ?1)",
            params![file],
        ) {
            let msg = format!("error: failed to delete stale edges for {}: {}", file, e);
            eprintln!("{}", msg);
            errors.push(msg);
        }
        if let Err(e) = self.db.execute("DELETE FROM symbols WHERE file = ?1", params![file]) {
            let msg = format!("error: failed to delete stale symbols for {}: {}", file, e);
            eprintln!("{}", msg);
            errors.push(msg);
        }
        if let Err(e) = self.db.execute("DELETE FROM files WHERE path = ?1", params![file]) {
            let msg = format!("error: failed to delete stale file record for {}: {}", file, e);
            eprintln!("{}", msg);
            errors.push(msg);
        }
        self.cache.remove(file);
    }

    /// `forget_file` for a file an earlier build stored; true when there was something to remove.
    fn forget_if_indexed(&mut self, file: &str, errors: &mut Vec<String>) -> bool {
        if !self.is_indexed(file) {
            return false;
        }
        self.forget_file(file, errors);
        true
    }

    pub fn build(&mut self, project_path: &str, options: &BuildOptions) -> BuildResult {
        let start = std::time::Instant::now();
        // In only_files mode, restrict to the requested files only
        let only_set: Option<HashSet<String>> = options.only_files.as_ref().map(|v| v.iter().cloned().collect());
        // Without the project directory there is no file list to compare the graph with: fail
        // the build and keep the graph as it is.
        let Collected { files, unreadable } = match self.collect_files(project_path, &options.exclude, only_set.as_ref()) {
            Ok(collected) => collected,
            Err(e) => {
                let msg = format!("error: cannot read the project directory {project_path}: {e}");
                eprintln!("{msg}");
                return BuildResult { duration_ms: start.elapsed().as_millis(), errors: vec![msg], ..BuildResult::default() };
            }
        };

        let mut processed = 0usize;
        let mut skipped = 0usize;
        let mut deleted = 0usize;
        let mut total_symbols = 0usize;
        let mut total_edges = 0usize;
        let mut errors: Vec<String> = Vec::new();

        // Clean up files no longer in scope (deleted, excluded, or filtered)
        {
            let current_paths: HashSet<String> = files.iter()
                .map(|f| pathdiff(f.full_path.to_str().unwrap_or(""), project_path))
                .collect();

            // In only_files mode: only check the targeted files for deletion (skip global stale scan)
            // In normal mode: scan all known files from cache + DB
            let all_known: HashSet<String> = if let Some(ref only) = only_set {
                only.clone()
            } else {
                let cached_paths = self.cache.known_files();
                let db_paths: Vec<String> = self.db
                    .prepare("SELECT DISTINCT path FROM files")
                    .and_then(|mut stmt| {
                        stmt.query_map([], |row| row.get(0))
                            .map(|rows| rows.filter_map(|r| r.ok()).collect())
                    })
                    .unwrap_or_default();
                let mut known: HashSet<String> = cached_paths.into_iter().collect();
                known.extend(db_paths);
                known
            };

            // A folder the walk couldn't read hides its files, which are not gone: they stay in
            // the graph as they are.
            for dir in &unreadable {
                eprintln!("warn: cannot read {dir}: its files stay in the graph as they were");
            }
            let under_unreadable = |path: &str| unreadable.iter().any(|dir| Path::new(path).starts_with(dir));

            for stale in &all_known {
                if !current_paths.contains(stale) && !under_unreadable(stale) {
                    self.forget_file(stale, &mut errors);
                    deleted += 1;
                }
            }
            if deleted > 0 {
                eprintln!("Cleaned up {} deleted/excluded files", deleted);
            }
        }

        eprintln!("Found {} source files", files.len());

        // Pass 1: Parse all files, insert symbols
        let mut pending_imports: Vec<(String, Vec<ParsedImport>)> = Vec::new();
        let mut pending_calls: Vec<(String, Vec<ParsedCallSite>)> = Vec::new();

        let log_interval = (files.len() / 10).max(1);

        for (idx, file) in files.iter().enumerate() {
            if options.is_cancelled() {
                eprintln!("Build cancelled.");
                break;
            }
            if (idx + 1) % log_interval == 0 || idx + 1 == files.len() {
                eprintln!("Parsing {}/{}...", idx + 1, files.len());
            }

            let rel_path = pathdiff(file.full_path.to_str().unwrap_or(""), project_path);

            // A file that isn't UTF-8 is skipped and named, and, like a minified file below, loses
            // what an earlier build stored for it. One that can't be read now (permissions) is
            // skipped with a warning and, like an unreadable folder, keeps what the graph has.
            let content = match fs::read_to_string(&file.read_path) {
                Ok(c) => c,
                Err(e) if e.kind() == std::io::ErrorKind::InvalidData => {
                    eprintln!("Skipped {}: not UTF-8", rel_path);
                    if self.forget_if_indexed(&rel_path, &mut errors) {
                        deleted += 1;
                    }
                    skipped += 1;
                    continue;
                }
                Err(e) => {
                    eprintln!("warn: cannot read {rel_path}: {e}; its records stay in the graph as they were");
                    skipped += 1;
                    continue;
                }
            };

            // Skip minified/bundled files: avg line > 500 chars with significant size. One indexed
            // before it was minified (or before this check) is still found on disk, so the stale
            // scan above keeps it: drop what an earlier build stored for it here.
            if content.len() > 5000 {
                let line_count = content.lines().count().max(1);
                if content.len() / line_count > 500 {
                    if self.forget_if_indexed(&rel_path, &mut errors) {
                        deleted += 1;
                    }
                    skipped += 1;
                    continue;
                }
            }

            // In only_files mode, force rebuild for targeted files (bypass cache check)
            let force = only_set.as_ref().map(|s| s.contains(&rel_path)).unwrap_or(false);
            if !options.full && !force && !self.cache.has_changed(&rel_path, &content) {
                skipped += 1;
                continue;
            }

            // Only this file's outgoing edges are rebuilt (pass 2 re-resolves its calls and
            // imports). Edges from other files into this one are kept: its symbols keep their ids
            // below, and edges to symbols that no longer exist go with them (ON DELETE CASCADE).
            if let Err(e) = self.db.execute(
                "DELETE FROM edges WHERE from_id IN (SELECT id FROM symbols WHERE file = ?1)",
                params![rel_path],
            ) {
                eprintln!("warn: failed to clean edges for {}: {}", rel_path, e);
            }
            let mut existing_ids = self.existing_symbol_ids(&rel_path);

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
                // A file the parser refuses is skipped and named like an unreadable one. Its outgoing
                // edges are already gone (above), so keeping its old symbols would leave half a file.
                match parse_single(file.full_path.to_str().unwrap_or(""), &content, &file.language) {
                    Ok(p) => p,
                    Err(e) => {
                        eprintln!("Skipped {}: {}", rel_path, e);
                        if self.forget_if_indexed(&rel_path, &mut errors) {
                            deleted += 1;
                        }
                        skipped += 1;
                        continue;
                    }
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

                // Same name and kind as a symbol already in this file (in source order): update
                // it in place, so edges from unchanged files that point at it stay valid.
                let reuse = existing_ids
                    .get_mut(&(sym.name.clone(), kind_str.to_string()))
                    .and_then(|ids| ids.pop_front());
                let written = match reuse {
                    Some(id) => self.db.execute(
                        "UPDATE symbols SET line_start = ?2, line_end = ?3, signature = ?4, doc = ?5, content = ?6 WHERE id = ?1",
                        params![id, sym.line_start, sym.line_end, sym.signature, sym.doc, body],
                    ),
                    None => self.db.execute(
                        "INSERT INTO symbols (name, kind, file, line_start, line_end, signature, doc, content) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                        params![sym.name, kind_str, rel_path, sym.line_start, sym.line_end, sym.signature, sym.doc, body],
                    ),
                };
                if let Err(e) = written {
                    eprintln!("warn: failed to insert symbol {} in {}: {}", sym.name, rel_path, e);
                    continue;
                }
                total_symbols += 1;
            }

            // Symbols removed from the file (their incoming edges cascade away).
            for id in existing_ids.into_values().flatten() {
                if let Err(e) = self.db.execute("DELETE FROM symbols WHERE id = ?1", params![id]) {
                    eprintln!("warn: failed to delete removed symbol in {}: {}", rel_path, e);
                }
            }

            if !parsed.call_sites.is_empty() {
                pending_calls.push((
                    rel_path.clone(),
                    parsed.call_sites.iter().map(|cs| ParsedCallSite {
                        caller: cs.caller.clone(),
                        callee: cs.callee.clone(),
                        receiver: cs.receiver.clone(),
                        kind: cs.kind.clone(),
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

                // 3. Receiver-scoped lookup (if receiver is a known class/module)
                let callee_id = callee_id.or_else(|| {
                    cs.receiver.as_ref().and_then(|recv| {
                        self.db.query_row(
                            "SELECT s.id FROM symbols s
                             JOIN symbols parent ON s.file = parent.file
                             WHERE s.name = ?1
                               AND parent.name = ?2
                               AND parent.kind IN ('Class', 'Interface', 'Module')
                               AND s.line_start >= parent.line_start
                               AND s.line_end <= parent.line_end",
                            params![cs.callee, recv],
                            |r| r.get(0),
                        ).ok()
                    })
                });

                // 4. Global fallback
                let (callee_id, confidence) = if let Some(id) = callee_id {
                    (Some(id), 1.0f64)
                } else {
                    let global_id: Option<i64> = self.db.query_row(
                        "SELECT id FROM symbols WHERE name = ?1",
                        params![cs.callee],
                        |r| r.get(0),
                    ).ok();
                    let conf = if cs.receiver.is_some() { 0.3f64 } else { 0.5f64 };
                    (global_id, conf)
                };

                if let Some(callee_id) = callee_id {
                    if let Err(e) = self.db.execute(
                        "INSERT INTO edges (from_id, to_id, kind, file, confidence) VALUES (?1, ?2, ?3, ?4, ?5)",
                        params![caller_id, callee_id, cs.kind.as_str(), rel_path, confidence],
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
            let msg = format!("error: failed to commit transaction: {}", e);
            eprintln!("{}", msg);
            errors.push(msg);
        }
        self.cache.save();

        BuildResult {
            files_processed: processed,
            symbols_found: total_symbols,
            edges_created: total_edges,
            skipped,
            deleted,
            duration_ms: start.elapsed().as_millis(),
            errors,
        }
    }

    pub fn database(&self) -> &Connection {
        &self.db
    }

    /// Source files under `project_path` (only those in `only`, when given), skipping:
    /// - anything git ignores: `.gitignore` files at any depth inside the project (not above
    ///   it), `.git/info/exclude` and the global gitignore, with full gitignore semantics
    ///   (`**`, anchoring, `!` negation), and `.ignore` files;
    /// - the project and global `exclude` patterns, which use the same gitignore syntax;
    /// - dotfiles and dot-directories;
    /// - whatever is not a regular file (FIFOs, sockets, devices);
    /// - symlinks, unless they resolve to a regular file inside the project that this walk
    ///   accepts itself, so a link can't bring in a file the rules above skip.
    ///
    /// Folders inside the project that can't be read are returned, relative to the root, with
    /// the files found elsewhere. Fails when the project directory can't be resolved or read.
    fn collect_files(&self, project_path: &str, exclude: &[String], only: Option<&HashSet<String>>) -> std::io::Result<Collected> {
        let root = PathBuf::from(project_path);
        let canonical_root = fs::canonicalize(&root)?;
        fs::read_dir(&root)?;
        let excludes = build_exclude_matcher(&root, exclude);
        let filter_root = root.clone();

        let walker = ignore::WalkBuilder::new(&root)
            .hidden(true)
            .git_ignore(true)
            .git_exclude(true)
            .git_global(true)
            // Only ignore files inside the project. Parent directories' files would let a
            // `~/.gitignore` with `*` (dotfiles setups) empty every project under $HOME, and
            // graphmind never read them before.
            .parents(false)
            // Honour .gitignore even when the project isn't (yet) a git repository, as before.
            .require_git(false)
            // Entries that are not symlinks are then inside the project by construction.
            .follow_links(false)
            .sort_by_file_name(|a, b| a.cmp(b))
            .filter_entry(move |entry| {
                let rel = entry.path().strip_prefix(&filter_root).unwrap_or(entry.path());
                if rel.as_os_str().is_empty() {
                    return true;
                }
                let is_dir = entry.file_type().is_some_and(|t| t.is_dir());
                !excludes.matched(rel, is_dir).is_ignore()
            })
            .build();

        // Every regular file the walk accepts, relative to the root: the targets a symlink may
        // resolve to.
        let mut accepted: HashSet<PathBuf> = HashSet::new();
        // (path, language, is a symlink), in walk order.
        let mut candidates = Vec::new();
        let mut unreadable = Vec::new();
        for entry in walker {
            let entry = match entry {
                Ok(entry) => entry,
                Err(err) => {
                    if let Some(path) = walk_error_path(&err) {
                        unreadable.push(pathdiff(path.to_str().unwrap_or(""), project_path));
                    }
                    continue;
                }
            };
            let Some(file_type) = entry.file_type() else { continue };
            let is_link = entry.path_is_symlink();
            if !is_link && !file_type.is_file() {
                continue;
            }
            if !is_link {
                accepted.insert(entry.path().strip_prefix(&root).unwrap_or(entry.path()).to_path_buf());
            }
            let name = entry.file_name().to_string_lossy().to_string();
            let ext = Path::new(&name).extension()
                .map(|e| format!(".{}", e.to_string_lossy()))
                .unwrap_or_default();
            let Some(language) = extension_to_language(&ext).or_else(|| extension_to_language(&name)) else {
                continue;
            };
            if only.is_some_and(|only| !only.contains(&pathdiff(entry.path().to_str().unwrap_or(""), project_path))) {
                continue;
            }
            candidates.push((entry.into_path(), language, is_link));
        }

        let files = candidates
            .into_iter()
            .filter_map(|(full_path, language, is_link)| {
                let read_path = if is_link {
                    let target = project_file(&canonical_root, &full_path)?;
                    accepted.contains(target.strip_prefix(&canonical_root).ok()?).then_some(target)?
                } else {
                    full_path.clone()
                };
                Some(SourceFile { full_path, read_path, language: language.to_string() })
            })
            .collect();
        Ok(Collected { files, unreadable })
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

/// Project and global exclude patterns as one gitignore-style matcher rooted at the project.
/// A plain name (`node_modules`, `secrets.yaml`) matches at any depth, a pattern with a slash
/// is relative to the project root, and `*`, `**` and `!` work as in `.gitignore`.
fn build_exclude_matcher(root: &Path, exclude: &[String]) -> ignore::gitignore::Gitignore {
    let mut builder = ignore::gitignore::GitignoreBuilder::new(root);
    for pattern in exclude {
        let pattern = pattern.trim();
        if pattern.is_empty() {
            continue;
        }
        if let Err(e) = builder.add_line(None, pattern) {
            eprintln!("warn: ignoring invalid exclude pattern {pattern:?}: {e}");
        }
    }
    builder.build().unwrap_or_else(|e| {
        eprintln!("warn: exclude patterns not applied: {e}");
        ignore::gitignore::Gitignore::empty()
    })
}

/// What a walk of the project found: the files to index, and the folders it couldn't read
/// (relative to the project root).
struct Collected {
    files: Vec<SourceFile>,
    unreadable: Vec<String>,
}

/// The path a walk error is about (the folder that couldn't be read), if it names one.
fn walk_error_path(err: &ignore::Error) -> Option<&Path> {
    match err {
        ignore::Error::WithPath { path, .. } => Some(path),
        ignore::Error::WithDepth { err, .. } | ignore::Error::WithLineNumber { err, .. } => walk_error_path(err),
        _ => None,
    }
}

fn pathdiff(full_path: &str, base: &str) -> String {
    let full = Path::new(full_path);
    let base = Path::new(base);
    full.strip_prefix(base)
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|_| full_path.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every language a file extension maps to has a parser: a mapped language without one would
    /// make the build skip each file of it ("Unsupported language"). Markdown has its own parser.
    #[test]
    fn every_mapped_language_has_a_parser() {
        for (ext, language) in LANGUAGE_MAP.iter().filter(|(_, l)| *l != "markdown") {
            assert!(
                parse_single("probe", "", language).is_ok(),
                "{ext} maps to {language}, which the parser doesn't support"
            );
        }
    }
}

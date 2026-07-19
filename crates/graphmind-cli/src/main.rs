use clap::Parser;

use graphmind_cli::commands;

#[derive(Parser)]
#[command(name = "graphmind", version, about = "Local-first code intelligence CLI")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(clap::Subcommand)]
enum Commands {
    /// Register a project
    Register {
        #[arg(default_value = ".")]
        path: String,
        #[arg(long)]
        slug: Option<String>,
        #[arg(long, num_args = 1..)]
        exclude: Vec<String>,
    },
    /// Unregister a project
    Unregister {
        slug: Option<String>,
    },
    /// List registered projects
    List,
    /// Show project status
    Status {
        slug: Option<String>,
    },
    /// Build the code graph
    Build {
        slug: Option<String>,
        #[arg(long)]
        all: bool,
        #[arg(long)]
        full: bool,
        #[arg(long)]
        reset: bool,
        #[arg(long)]
        watch: bool,
    },
    /// Query a symbol
    Query {
        name: String,
        #[arg(long, alias = "in")]
        slug: Option<String>,
        /// Filter by file path
        #[arg(long)]
        file: Option<String>,
        /// Filter by symbol kind (Function, Method, Class, Interface, Type)
        #[arg(long)]
        kind: Option<String>,
        /// Max callers/callees to show (default 15)
        #[arg(long, default_value = "15")]
        limit: usize,
        /// Skip first N callers/callees (default 0)
        #[arg(long, default_value = "0")]
        offset: usize,
    },
    /// Show function/symbol detail with source
    Fn {
        name: String,
        #[arg(long, alias = "in")]
        slug: Option<String>,
        #[arg(long)]
        no_tests: bool,
        /// Filter by file path to disambiguate common symbol names
        #[arg(long)]
        file: Option<String>,
        /// Filter by symbol kind (Function, Method, Class, Interface, Type)
        #[arg(long)]
        kind: Option<String>,
        /// Max callers/callees to show (default 15)
        #[arg(long, default_value = "15")]
        limit: usize,
        /// Skip first N callers/callees (default 0)
        #[arg(long, default_value = "0")]
        offset: usize,
        /// Show source content
        #[arg(long)]
        include_content: bool,
    },
    /// Show file dependencies
    Deps {
        file: String,
        #[arg(long, alias = "in")]
        slug: Option<String>,
    },
    /// Show impact of changes to a file
    Impact {
        file: String,
        #[arg(long, alias = "in")]
        slug: Option<String>,
        #[arg(long, default_value = "3")]
        depth: usize,
    },
    /// Show impact of changes to a symbol
    FnImpact {
        name: String,
        #[arg(long, alias = "in")]
        slug: Option<String>,
        #[arg(long, default_value = "3")]
        depth: usize,
        /// Filter by file path to disambiguate common symbol names
        #[arg(long)]
        file: Option<String>,
    },
    /// Show graph map overview
    Map {
        slug: Option<String>,
    },
    /// Detect dependency cycles
    Cycles {
        slug: Option<String>,
    },
    /// Search symbols by text
    Search {
        query: String,
        #[arg(long, alias = "in")]
        slug: Option<String>,
        #[arg(long, default_value = "20")]
        limit: i64,
        #[arg(long)]
        kind: Option<String>,
        /// Skip first N results (default 0)
        #[arg(long, default_value = "0")]
        offset: usize,
        /// Show source content in results
        #[arg(long)]
        include_content: bool,
    },
    /// Show file outline (hierarchical symbol tree)
    Outline {
        file: String,
        #[arg(long, alias = "in")]
        slug: Option<String>,
    },
    /// Read raw file content from a registered project
    File {
        file: String,
        #[arg(long, alias = "in")]
        slug: Option<String>,
    },
    /// Trace transitive callers of a symbol
    WhoCalls {
        symbol: String,
        #[arg(long, alias = "in")]
        slug: Option<String>,
        /// Max depth to trace (default 3)
        #[arg(long, default_value = "3")]
        depth: usize,
    },
    /// Find unreachable symbols (dead code)
    DeadCode {
        #[arg(long, alias = "in")]
        slug: Option<String>,
        /// Filter by kind (Function, Method, Class, ...)
        #[arg(long)]
        kind: Option<String>,
        /// Max results (default 50)
        #[arg(long, default_value = "50")]
        limit: i64,
    },
    /// Find structurally similar symbols
    Similar {
        symbol: String,
        #[arg(long, alias = "in")]
        slug: Option<String>,
        /// Max results (default 10)
        #[arg(long, default_value = "10")]
        limit: i64,
    },
    /// Find listeners for an event
    Listeners {
        event: String,
        #[arg(long, alias = "in")]
        slug: Option<String>,
    },
    /// Show embedding index status or generate embeddings
    Embed {
        #[arg(long)]
        run: bool,
        #[arg(long)]
        all: bool,
        slug: Option<String>,
    },
    /// Session logging and context
    Session {
        #[command(subcommand)]
        action: SessionAction,
    },
    /// Install components (hooks, skill)
    Install {
        #[command(subcommand)]
        action: InstallAction,
    },
    /// Uninstall components (hooks)
    Uninstall {
        #[command(subcommand)]
        action: UninstallAction,
    },
    /// Memory operations
    Memory {
        #[command(subcommand)]
        action: MemoryAction,
    },
    /// Cross-project operations
    Cross {
        #[command(subcommand)]
        action: CrossAction,
    },
    /// Clean graph data
    Clean {
        slug: Option<String>,
        #[arg(long)]
        all: bool,
    },
    /// Sync CLAUDE.md with graph stats
    Sync {
        slug: Option<String>,
        #[arg(long)]
        all: bool,
        #[arg(long)]
        dir: Option<String>,
    },
    /// Show impact of git diff changes
    DiffImpact {
        #[arg(long, alias = "in")]
        slug: Option<String>,
        #[arg(long)]
        staged: bool,
        #[arg(long, default_value = "3")]
        depth: usize,
    },
    /// Manage exclude patterns
    Exclude {
        #[command(subcommand)]
        action: ExcludeAction,
    },
    /// Export graph in various formats
    Export {
        slug: Option<String>,
        #[arg(short, long, default_value = "json")]
        format: String,
        #[arg(long)]
        cross: bool,
        #[arg(long)]
        obsidian: Option<String>,
    },
    /// Global setup: Claude Code hooks, MCP configs, skill (run once)
    Setup,
    /// Initialize a project: register, git hooks, build (run per project)
    Init {
        #[arg(default_value = ".")]
        path: String,
        #[arg(long)]
        skip_build: bool,
    },
    /// Update graphmind to the latest version
    Update {
        #[arg(long)]
        check: bool,
    },
    /// Start MCP server
    Mcp,
    /// Manage licence (login, status, logout)
    Auth {
        #[command(subcommand)]
        action: AuthAction,
    },
}

#[derive(clap::Subcommand)]
enum AuthAction {
    /// Activate a licence key
    Login {
        #[arg(long)]
        key: String,
    },
    /// Show current licence status
    Status,
    /// Remove licence key (revert to Free)
    Logout,
}

#[derive(clap::Subcommand)]
enum MemoryAction {
    /// Add a memory entry
    Add {
        content: String,
        #[arg(long, alias = "in")]
        slug: Option<String>,
        #[arg(long)]
        global: bool,
        #[arg(long, num_args = 1..)]
        tags: Vec<String>,
        #[arg(long, default_value = "context")]
        r#type: String,
        #[arg(long)]
        priority: bool,
    },
    /// Search memories
    Search {
        query: String,
        #[arg(long, alias = "in")]
        slug: Option<String>,
        #[arg(long, default_value = "10")]
        limit: usize,
    },
    /// List memories
    List {
        #[arg(long, alias = "in")]
        slug: Option<String>,
        #[arg(long, default_value = "20")]
        limit: usize,
        #[arg(long)]
        priority: bool,
        /// Run clean (expire, dedup, auto-promote) before listing
        #[arg(long)]
        clean: bool,
    },
    /// Delete a memory entry
    Delete {
        id: String,
        #[arg(long, alias = "in")]
        slug: Option<String>,
    },
    /// Remove expired entries, [commit] noise, duplicates, and auto-promote high-recall entries
    Clean {
        #[arg(long, alias = "in")]
        slug: Option<String>,
    },
    /// Full consolidate: expire, purge noise, dedup, auto-promote
    Consolidate {
        #[arg(long, alias = "in")]
        slug: Option<String>,
        /// Dry-run: show what would be done without writing anything
        #[arg(long)]
        dry_run: bool,
    },
}

#[derive(clap::Subcommand)]
enum CrossAction {
    /// Search a symbol across ALL projects
    Query {
        symbol: String,
    },
    /// Show cross-project dependencies
    Deps {
        slug: Option<String>,
    },
    /// List all cross-links
    Links,
    /// Add a cross-link
    Link {
        #[command(subcommand)]
        action: CrossLinkAction,
    },
}

#[derive(clap::Subcommand)]
enum CrossLinkAction {
    /// Add a manual cross-link
    Add {
        from: String,
        to: String,
        #[arg(long, default_value = "shares-pattern")]
        r#type: String,
        #[arg(long, default_value = "manual link")]
        reason: String,
    },
    /// Infer cross-links from shared symbols
    Infer,
}

#[derive(clap::Subcommand)]
enum SessionAction {
    /// Log session start and show context
    Start {
        slug: Option<String>,
    },
    /// Save session summary
    Save {
        message: Option<String>,
        #[arg(long, alias = "in")]
        slug: Option<String>,
    },
    /// Show recent session entries
    History {
        slug: Option<String>,
        #[arg(short = 'n', long, default_value = "10")]
        limit: usize,
    },
}

#[derive(clap::Subcommand)]
enum InstallAction {
    /// Install Claude Code search hook (redirects grep/find to graphmind)
    HookClaude,
    /// Install git hooks (post-commit + pre-push)
    HookGit {
        slug: Option<String>,
    },
    /// Install Claude Code skill
    Skill,
}

#[derive(clap::Subcommand)]
enum UninstallAction {
    /// Remove Claude Code search hook
    HookClaude,
    /// Remove graphmind git hooks
    HookGit {
        slug: Option<String>,
    },
    /// Remove all graphmind integrations (reverses setup + init)
    All {
        #[arg(long)]
        purge: bool,
        #[arg(long)]
        yes: bool,
    },
}

#[derive(clap::Subcommand)]
enum ExcludeAction {
    /// Add exclude patterns
    Add {
        #[arg(num_args = 1..)]
        patterns: Vec<String>,
        #[arg(long, alias = "in")]
        slug: Option<String>,
        #[arg(long)]
        global: bool,
    },
    /// Remove exclude patterns
    Remove {
        #[arg(num_args = 1..)]
        patterns: Vec<String>,
        #[arg(long, alias = "in")]
        slug: Option<String>,
        #[arg(long)]
        global: bool,
    },
    /// List exclude patterns
    List {
        #[arg(long, alias = "in")]
        slug: Option<String>,
        #[arg(long)]
        global: bool,
    },
}

fn main() {
    let cli = Cli::parse();
    graphmind_config::ensure_dirs();

    let skip_notices = std::env::var("GRAPHMIND_SKIP_NOTICES").is_ok();

    // Show notices (setup outdated, announcements) except for setup/mcp/update commands
    if !skip_notices && !matches!(cli.command, Commands::Setup | Commands::Mcp | Commands::Update { .. }) {
        commands::notices::check_setup_version();
        commands::notices::check_announcements();
        commands::notices::check_cli_update();
        commands::notices::check_schema_version();
        commands::notices::memory_auto_clean();
        // Auto-reinstall hooks/skills/CLAUDE.md when binary is newer than installed setup version
        commands::notices::auto_setup_if_outdated();
    }

    // Silent license revalidation every 24h (non-blocking)
    if !skip_notices && !matches!(cli.command, Commands::Auth { .. }) {
        std::thread::spawn(commands::auth::maybe_revalidate);
    }

    match cli.command {
        Commands::Register {
            path,
            slug,
            exclude,
        } => {
            commands::register::register(&path, slug.as_deref(), &exclude);
        }
        Commands::Unregister { slug } => {
            commands::register::unregister(slug.as_deref());
        }
        Commands::List => {
            commands::register::list();
        }
        Commands::Status { slug } => {
            commands::register::status(slug.as_deref());
        }
        Commands::Build {
            slug,
            all,
            full,
            reset,
            watch,
        } => {
            commands::build::build(slug.as_deref(), all, full, reset, watch);
        }
        Commands::Query { name, slug, file, kind, limit, offset } => {
            commands::query::query_symbol(&name, slug.as_deref(), file.as_deref(), kind.as_deref(), limit, offset);
        }
        Commands::Fn {
            name,
            slug,
            no_tests,
            file,
            kind,
            limit,
            offset,
            include_content,
        } => {
            commands::query::fn_detail(&name, slug.as_deref(), &commands::query::FnDetailOpts {
                no_tests,
                file: file.as_deref(),
                kind: kind.as_deref(),
                limit,
                offset,
                include_content,
            });
        }
        Commands::Deps { file, slug } => {
            commands::query::deps(&file, slug.as_deref());
        }
        Commands::Impact { file, slug, depth } => {
            commands::query::impact(&file, slug.as_deref(), depth);
        }
        Commands::FnImpact { name, slug, depth, file } => {
            commands::query::fn_impact(&name, slug.as_deref(), depth, file.as_deref());
        }
        Commands::Map { slug } => {
            commands::query::map(slug.as_deref());
        }
        Commands::Cycles { slug } => {
            commands::query::cycles(slug.as_deref());
        }
        Commands::Search { query, slug, limit, kind, offset, include_content } => {
            commands::search::search(&query, slug.as_deref(), limit, kind.as_deref(), offset, include_content);
        }
        Commands::Outline { file, slug } => {
            commands::query::outline(&file, slug.as_deref());
        }
        Commands::File { file, slug } => {
            commands::query::file_content(&file, slug.as_deref());
        }
        Commands::WhoCalls { symbol, slug, depth } => {
            commands::query::who_calls(&symbol, slug.as_deref(), depth);
        }
        Commands::DeadCode { slug, kind, limit } => {
            commands::query::dead_code(slug.as_deref(), kind.as_deref(), limit);
        }
        Commands::Similar { symbol, slug, limit } => {
            commands::query::similar(&symbol, slug.as_deref(), limit);
        }
        Commands::Listeners { event, slug } => {
            commands::query::listeners(&event, slug.as_deref());
        }
        Commands::Embed { run, all, slug } => {
            if run {
                commands::build::embed_only(slug.as_deref(), all);
            } else {
                commands::search::embed_status(slug.as_deref());
            }
        }
        Commands::Session { action } => match action {
            SessionAction::Start { slug } => {
                commands::session::start(slug.as_deref());
            }
            SessionAction::Save { message, slug } => {
                commands::session::save(message.as_deref(), slug.as_deref());
            }
            SessionAction::History { slug, limit } => {
                commands::session::history(slug.as_deref(), limit);
            }
        },
        Commands::Install { action } => match action {
            InstallAction::HookClaude => {
                commands::claude_hook::install_hook();
            }
            InstallAction::HookGit { slug } => {
                commands::hooks::install(slug.as_deref());
            }
            InstallAction::Skill => {
                commands::install_skill::install_skill();
            }
        },
        Commands::Uninstall { action } => match action {
            UninstallAction::HookClaude => {
                commands::claude_hook::uninstall_hook();
            }
            UninstallAction::HookGit { slug } => {
                commands::hooks::uninstall(slug.as_deref());
            }
            UninstallAction::All { purge, yes } => {
                commands::setup::uninstall_all(purge, yes);
            }
        },
        Commands::Memory { action } => match action {
            MemoryAction::Add {
                content,
                slug,
                global,
                tags,
                r#type,
                priority,
            } => {
                commands::memory::add(&content, slug.as_deref(), global, &tags, &r#type, priority);
            }
            MemoryAction::Search { query, slug, limit } => {
                commands::memory::search(&query, slug.as_deref(), limit);
            }
            MemoryAction::List { slug, limit, priority, clean } => {
                commands::memory::list(slug.as_deref(), limit, priority, clean);
            }
            MemoryAction::Delete { id, slug } => {
                commands::memory::delete(&id, slug.as_deref());
            }
            MemoryAction::Clean { slug } => {
                commands::memory::clean(slug.as_deref());
            }
            MemoryAction::Consolidate { slug, dry_run } => {
                commands::memory::consolidate(slug.as_deref(), dry_run);
            }
        },
        Commands::Cross { action } => match action {
            CrossAction::Query { symbol } => {
                commands::cross::cross_query(&symbol);
            }
            CrossAction::Deps { slug } => {
                commands::cross::cross_deps(slug.as_deref());
            }
            CrossAction::Links => {
                commands::cross::cross_links();
            }
            CrossAction::Link { action } => match action {
                CrossLinkAction::Add {
                    from,
                    to,
                    r#type,
                    reason,
                } => {
                    commands::cross::cross_link_add(&from, &to, &r#type, &reason);
                }
                CrossLinkAction::Infer => {
                    commands::cross::cross_link_infer();
                }
            },
        },
        Commands::Clean { slug, all } => {
            commands::clean::clean(slug.as_deref(), all);
        }
        Commands::Sync { slug, all, dir } => {
            commands::sync::sync(slug.as_deref(), all, dir.as_deref());
        }
        Commands::DiffImpact {
            slug,
            staged,
            depth,
        } => {
            commands::diff_impact::diff_impact(slug.as_deref(), staged, depth);
        }
        Commands::Exclude { action } => match action {
            ExcludeAction::Add {
                patterns,
                slug,
                global,
            } => {
                commands::exclude::add(&patterns, slug.as_deref(), global);
            }
            ExcludeAction::Remove {
                patterns,
                slug,
                global,
            } => {
                commands::exclude::remove(&patterns, slug.as_deref(), global);
            }
            ExcludeAction::List { slug, global } => {
                commands::exclude::list(slug.as_deref(), global);
            }
        },
        Commands::Export {
            slug,
            format,
            cross,
            obsidian,
        } => {
            commands::export::export(slug.as_deref(), &format, cross, obsidian.as_deref());
        }
        Commands::Setup => {
            commands::setup::setup();
        }
        Commands::Init { path, skip_build } => {
            commands::setup::init(Some(&path), skip_build);
        }
        Commands::Update { check } => {
            commands::update::update(check);
        }
        Commands::Mcp => {
            let rt = tokio::runtime::Runtime::new().expect("failed to create tokio runtime");
            if let Err(e) = rt.block_on(graphmind_mcp::server::run_mcp_server()) {
                eprintln!("MCP server error: {e}");
                std::process::exit(1);
            }
        }
        Commands::Auth { action } => match action {
            AuthAction::Login { key } => {
                commands::auth::login(&key);
            }
            AuthAction::Status => {
                commands::auth::status();
            }
            AuthAction::Logout => {
                commands::auth::logout();
            }
        },
    }
}

use clap::Parser;

mod commands;

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
        watch: bool,
    },
    /// Query a symbol
    Query {
        name: String,
        #[arg(long, alias = "in")]
        slug: Option<String>,
    },
    /// Show function/symbol detail with source
    Fn {
        name: String,
        #[arg(long, alias = "in")]
        slug: Option<String>,
        #[arg(long)]
        no_tests: bool,
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
    },
    /// Show embedding index status
    Embed,
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
    },
    /// Delete a memory entry
    Delete {
        id: String,
        #[arg(long, alias = "in")]
        slug: Option<String>,
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
            watch,
        } => {
            commands::build::build(slug.as_deref(), all, full, watch);
        }
        Commands::Query { name, slug } => {
            commands::query::query_symbol(&name, slug.as_deref());
        }
        Commands::Fn {
            name,
            slug,
            no_tests,
        } => {
            commands::query::fn_detail(&name, slug.as_deref(), no_tests);
        }
        Commands::Deps { file, slug } => {
            commands::query::deps(&file, slug.as_deref());
        }
        Commands::Impact { file, slug, depth } => {
            commands::query::impact(&file, slug.as_deref(), depth);
        }
        Commands::FnImpact { name, slug, depth } => {
            commands::query::fn_impact(&name, slug.as_deref(), depth);
        }
        Commands::Map { slug } => {
            commands::query::map(slug.as_deref());
        }
        Commands::Cycles { slug } => {
            commands::query::cycles(slug.as_deref());
        }
        Commands::Search { query, slug, limit, kind } => {
            commands::search::search(&query, slug.as_deref(), limit, kind.as_deref());
        }
        Commands::Embed => {
            commands::search::embed_status(None);
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
        },
        Commands::Memory { action } => match action {
            MemoryAction::Add {
                content,
                slug,
                global,
                tags,
                r#type,
            } => {
                commands::memory::add(&content, slug.as_deref(), global, &tags, &r#type);
            }
            MemoryAction::Search { query, slug, limit } => {
                commands::memory::search(&query, slug.as_deref(), limit);
            }
            MemoryAction::List { slug, limit } => {
                commands::memory::list(slug.as_deref(), limit);
            }
            MemoryAction::Delete { id, slug } => {
                commands::memory::delete(&id, slug.as_deref());
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
    }
}

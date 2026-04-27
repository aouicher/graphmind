use clap::Parser;

mod commands;
mod config;
mod paths;
mod resolve;

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
    },
    /// Generate embeddings (coming soon)
    Embed,
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
        obsidian: bool,
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
    /// Query cross-links for a project
    Query {
        slug: Option<String>,
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
    config::ensure_dirs();

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
        Commands::Search { query, slug, limit } => {
            commands::search::search(&query, slug.as_deref(), limit);
        }
        Commands::Embed => {
            commands::search::embed();
        }
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
            CrossAction::Query { slug } => {
                commands::cross::cross_query(slug.as_deref());
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
            commands::diff_impact::diff_impact(None, slug.as_deref(), staged, depth);
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
            commands::export::export(slug.as_deref(), &format, cross, obsidian);
        }
        Commands::Mcp => {
            println!("MCP server coming soon");
        }
    }
}

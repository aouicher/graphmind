use clap::Parser;

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
    /// Start MCP server
    Mcp,
    /// Show project status
    Status,
    /// List registered projects
    List,
}

fn main() {
    let cli = Cli::parse();

    match cli.command {
        Commands::Register { path, slug } => {
            println!("register: path={path} slug={slug:?}");
        }
        Commands::Build { slug, all, full, watch } => {
            println!("build: slug={slug:?} all={all} full={full} watch={watch}");
        }
        Commands::Mcp => {
            println!("mcp server (stub)");
        }
        Commands::Status => {
            println!("status (stub)");
        }
        Commands::List => {
            println!("list (stub)");
        }
    }
}

// OpenPawz CLI — Command-line interface to the OpenPawz AI engine.
//
// Talks directly to the openpawz-core library (same Rust code as the desktop app)
// with zero network overhead. Shares the same SQLite database and config.

mod commands;

use clap::{CommandFactory, Parser, Subcommand};
use clap_complete::{generate, Shell};

const BANNER: &str = concat!(
    "\n",
    "\x1b[1;38;5;208m   ██████╗ ██████╗ ███████╗███╗   ██╗\x1b[0m\n",
    "\x1b[1;38;5;208m  ██╔═══██╗██╔══██╗██╔════╝████╗  ██║\x1b[0m\n",
    "\x1b[1;38;5;209m  ██║   ██║██████╔╝█████╗  ██╔██╗ ██║\x1b[0m\n",
    "\x1b[1;38;5;209m  ██║   ██║██╔═══╝ ██╔══╝  ██║╚██╗██║\x1b[0m\n",
    "\x1b[1;38;5;210m  ╚██████╔╝██║     ███████╗██║ ╚████║\x1b[0m\n",
    "\x1b[1;38;5;210m   ╚═════╝ ╚═╝     ╚══════╝╚═╝  ╚═══╝\x1b[0m\n",
    "\x1b[1;38;5;215m  ██████╗  █████╗ ██╗    ██╗███████╗\x1b[0m\n",
    "\x1b[1;38;5;215m  ██╔══██╗██╔══██╗██║    ██║╚══███╔╝\x1b[0m\n",
    "\x1b[1;38;5;216m  ██████╔╝███████║██║ █╗ ██║  ███╔╝\x1b[0m\n",
    "\x1b[1;38;5;216m  ██╔═══╝ ██╔══██║██║███╗██║ ███╔╝\x1b[0m\n",
    "\x1b[1;38;5;217m  ██║     ██║  ██║╚███╔███╔╝███████╗\x1b[0m\n",
    "\x1b[1;38;5;217m  ╚═╝     ╚═╝  ╚═╝ ╚══╝╚══╝ ╚══════╝\x1b[0m\n",
    "\n",
    "\x1b[38;5;240m  ──────────────────────────────────────\x1b[0m\n",
    "  \x1b[38;5;250m🐾 Multi-Agent AI from the Terminal\x1b[0m\n"
);

#[derive(Parser)]
#[command(
    name = "openpawz",
    about = "Multi-agent AI from the terminal",
    version,
    arg_required_else_help = true,
    before_help = BANNER
)]
struct Cli {
    /// Output format
    #[arg(long, global = true, default_value = "human")]
    output: OutputFormat,

    /// Enable verbose logging
    #[arg(long, short, global = true)]
    verbose: bool,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Clone, clap::ValueEnum)]
enum OutputFormat {
    Human,
    Json,
    Quiet,
}

#[derive(Subcommand)]
enum Commands {
    /// Manage agents (list, create, delete, files, context)
    Agent {
        #[command(subcommand)]
        action: commands::agent::AgentAction,
    },
    /// Manage chat sessions (list, delete, history)
    Session {
        #[command(subcommand)]
        action: commands::session::SessionAction,
    },
    /// Engine configuration (get, set)
    Config {
        #[command(subcommand)]
        action: commands::config::ConfigAction,
    },
    /// Memory operations (store, search, list, export, import)
    Memory {
        #[command(subcommand)]
        action: commands::memory::MemoryAction,
    },
    /// Task management (list, create, update, delete, due)
    Task {
        #[command(subcommand)]
        action: commands::task::TaskAction,
    },
    /// Tamper-evident audit log (log, verify, stats)
    Audit {
        #[command(subcommand)]
        action: commands::audit::AuditAction,
    },
    /// Multi-agent project orchestration (list, create, team, messages)
    Project {
        #[command(subcommand)]
        action: commands::project::ProjectAction,
    },
    /// Engine status and diagnostics
    Status,
    /// Comprehensive health check
    Doctor,
    /// Initial setup wizard
    Setup,
    /// Generate shell completions
    Completions {
        /// Shell to generate completions for
        #[arg(value_enum)]
        shell: Shell,
    },
}

#[tokio::main]
async fn main() {
    let cli = Cli::parse();

    if cli.verbose {
        env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("debug")).init();
    } else {
        env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("warn")).init();
    }

    // Initialize the core engine (loads DB, paths, key vault)
    openpawz_core::engine::paths::load_data_root_from_conf();

    let store = match openpawz_core::engine::sessions::SessionStore::open() {
        Ok(s) => s,
        Err(e) => {
            eprintln!("Error: Failed to open database: {}", e);
            std::process::exit(1);
        }
    };

    let result = match cli.command {
        Commands::Agent { action } => commands::agent::run(&store, action, &cli.output),
        Commands::Session { action } => commands::session::run(&store, action, &cli.output),
        Commands::Config { action } => commands::config::run(&store, action, &cli.output),
        Commands::Memory { action } => commands::memory::run(&store, action, &cli.output).await,
        Commands::Task { action } => commands::task::run(&store, action, &cli.output),
        Commands::Audit { action } => commands::audit::run(&store, action, &cli.output),
        Commands::Project { action } => commands::project::run(&store, action, &cli.output),
        Commands::Status => commands::status::run(&store, &cli.output),
        Commands::Doctor => commands::doctor::run(&store, &cli.output),
        Commands::Setup => commands::setup::run(&store),
        Commands::Completions { shell } => {
            let mut cmd = Cli::command();
            generate(shell, &mut cmd, "openpawz", &mut std::io::stdout());
            Ok(())
        }
    };

    if let Err(e) = result {
        eprintln!("Error: {}", e);
        std::process::exit(1);
    }
}

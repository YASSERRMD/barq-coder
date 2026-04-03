use clap::{Parser, Subcommand};

/// Barq Coder — Autonomous Coding Agent
#[derive(Parser, Debug, Clone)]
#[command(
    name = "barqcoder",
    version = env!("CARGO_PKG_VERSION"),
    about = "Autonomous coding agent powered by local LLMs via Ollama",
    long_about = None,
)]
pub struct Cli {
    /// Subcommand to run (optional; defaults to interactive TUI)
    #[command(subcommand)]
    pub command: Option<Commands>,

    /// Override the Ollama model at runtime
    #[arg(long, global = true, env = "BARQ_MODEL")]
    pub model: Option<String>,

    /// Override the workspace root directory
    #[arg(long, global = true, env = "BARQ_WORKSPACE")]
    pub workspace: Option<String>,

    /// Override the Ollama base URL
    #[arg(long, global = true, env = "OLLAMA_BASE_URL")]
    pub ollama_url: Option<String>,

    /// Maximum tool-call iterations per turn
    #[arg(long, global = true, env = "BARQ_MAX_TURNS")]
    pub max_turns: Option<u8>,

    /// Resume the last session in this workspace
    #[arg(long, global = true, conflicts_with = "resume")]
    pub r#continue: bool,

    /// Resume a specific session by ID
    #[arg(long, global = true, value_name = "SESSION_ID")]
    pub resume: Option<String>,

    /// Skip permission prompts (dangerous — for CI/headless use only)
    #[arg(long, global = true)]
    pub dangerously_skip_permissions: bool,

    /// Start the LSP server instead of TUI
    #[arg(long, global = true)]
    pub lsp: bool,
}

#[derive(Subcommand, Debug, Clone)]
pub enum Commands {
    /// Run a single prompt headlessly and print the response (no TUI)
    #[command(alias = "p")]
    Print {
        /// The prompt to send to the agent
        prompt: String,

        /// Output in JSON format (for SDK/pipe integration)
        #[arg(long)]
        json: bool,
    },

    /// Index the workspace into BARQ semantic database
    Index {
        /// Path to index (defaults to workspace root)
        path: Option<String>,
    },

    /// Show session statistics and history
    Sessions {
        /// Show full event list for a specific session
        #[arg(long, value_name = "SESSION_ID")]
        show: Option<String>,

        /// Delete a specific session
        #[arg(long, value_name = "SESSION_ID")]
        delete: Option<String>,
    },

    /// Show or edit project memory (.barqcoder.md)
    Memory {
        /// Append a note to project memory
        #[arg(long, value_name = "NOTE")]
        add: Option<String>,

        /// Show all memory entries
        #[arg(long)]
        show: bool,
    },

    /// Check the agent's connection to Ollama
    Doctor,
}

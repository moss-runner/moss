//! Command-line interface definitions for Moss.
//!
//! All subcommands and flags are declared here using `clap`'s derive API.
//! Keeping the argument schema in its own file makes `main.rs` shorter and
//! makes it easy to add new subcommands without touching the dispatch logic.
use clap::{Parser, Subcommand};
use clap_complete::Shell;

// ── Top-level CLI ─────────────────────────────────────────────────────────────

/// Moss — a modern task runner.
///
/// Reads task definitions from a `Mossfile` in the current directory.
#[derive(Parser, Debug)]
#[command(
    name    = "moss",
    version = env!("CARGO_PKG_VERSION"),
    about   = "A fast, modern task runner",
    long_about = concat!(
        "Moss reads task definitions from a Mossfile in the current\n",
        "directory and executes them with built-in support for parallel\n",
        "execution, file watching, and dependency resolution."
    ),
)]
pub struct Cli {
    /// Path to the Mossfile (default: `./Mossfile`).
    #[arg(short, long, global = true, default_value = "Mossfile")]
    pub file: String,

    /// Print each shell command before running it.
    #[arg(short, long, global = true)]
    pub verbose: bool,

    #[command(subcommand)]
    pub command: Commands,
}

// ── Subcommands ───────────────────────────────────────────────────────────────

#[derive(Subcommand, Debug)]
pub enum Commands {
    /// Run a task defined in the Mossfile.
    ///
    /// Examples:
    ///   moss run build
    ///   moss run deploy production
    Run {
        /// Name of the task to run.
        task: String,

        /// Arguments passed to the task (used with `args=[…]` declarations).
        #[arg(trailing_var_arg = true)]
        args: Vec<String>,
    },

    /// List all tasks defined in the Mossfile.
    List,

    /// Generate shell completion scripts.
    ///
    /// Examples:
    ///   moss completions bash >> ~/.bashrc
    ///   moss completions zsh  >> ~/.zshrc
    Completions {
        /// Shell to generate completions for.
        #[arg(value_enum)]
        shell: Shell,
    },
}

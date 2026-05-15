//! Moss CLI entry point.
//!
//! Responsibilities:
//!   1. Parse command-line arguments via [`Cli`].
//!   2. Load and parse the `Mossfile`.
//!   3. Dispatch to the appropriate handler (`run`, `list`, `completions`).
//!   4. Print user-friendly errors and exit with a non-zero code on failure.
mod cli;
mod completions;
mod output;

use std::{fs, process};

use clap::Parser;
use moss_core::runner::{Runner, RunnerConfig};
use moss_parser::parse;

use cli::{Cli, Commands};
use output::TaskEntry;

// ── Entry point ───────────────────────────────────────────────────────────────

#[tokio::main]
async fn main() {
    let cli = Cli::parse();

    // Print the version banner for all commands except completions
    // (completions must write clean output with no extra text).
    if !matches!(cli.command, Commands::Completions { .. }) {
        output::version_banner();
    }

    match &cli.command {
        Commands::Completions { shell } => {
            completions::generate_completions(*shell);
        }

        Commands::List => {
            let mossfile = load_mossfile(&cli.file);
            cmd_list(&mossfile);
        }

        Commands::Run { task, args } => {
            let mossfile = load_mossfile(&cli.file);
            cmd_run(&mossfile, task, args, cli.verbose).await;
        }
    }
}

// ── Subcommand handlers ───────────────────────────────────────────────────────

/// Handle `moss list` — print all tasks with their flags and descriptions.
fn cmd_list(mossfile: &moss_parser::Mossfile) {
    let entries: Vec<TaskEntry<'_>> = mossfile
        .tasks
        .iter()
        .map(|task| {
            // Collect active flags into short labels for the flag column.
            let mut flags: Vec<&'static str> = Vec::new();
            if task.flags.parallel {
                flags.push("par");
            }
            if task.flags.watch.is_some() {
                flags.push("wat");
            }
            if task.flags.ready.is_some() {
                flags.push("rdy");
            }
            if !task.flags.deps.is_empty() {
                flags.push("dep");
            }
            if !task.flags.args.is_empty() {
                flags.push("arg");
            }

            TaskEntry {
                name: &task.name,
                description: task.description.as_deref(),
                flags,
            }
        })
        .collect();

    output::task_list(&entries);
    output::success(&format!("{} task(s) found", entries.len()));
}

/// Handle `moss run <task> [args…]` — execute a task and all its dependencies.
async fn cmd_run(
    mossfile: &moss_parser::Mossfile,
    task_name: &str,
    args: &[String],
    verbose: bool,
) {
    output::task_start(task_name);

    let config = RunnerConfig {
        shell: mossfile
            .settings
            .shell
            .clone()
            .unwrap_or_else(default_shell),
        echo_commands: verbose,
    };

    let runner = Runner::new(mossfile).with_config(config);
    let started = std::time::Instant::now();

    match runner.run(task_name, args).await {
        Ok(()) => {
            output::task_done(task_name, started.elapsed());
        }
        Err(e) => {
            output::task_failed(task_name);
            output::error(&e.to_string());
            process::exit(1);
        }
    }
}

// ── Helpers ───────────────────────────────────────────────────────────────────

/// Read and parse the Mossfile at `path`.
///
/// Exits the process with a user-friendly error if the file cannot be read
/// or parsed — no panic, no raw Rust error dumped to the terminal.
fn load_mossfile(path: &str) -> moss_parser::Mossfile {
    // Warn the user if they are loading from a non-default path.
    if path != "Mossfile" {
        output::warn(&format!("using Mossfile at `{}`", path));
    }

    // Read the file.
    let src = fs::read_to_string(path).unwrap_or_else(|e| {
        output::error(&format!("cannot read `{}`: {}", path, e));
        output::info("make sure a Mossfile exists in the current directory");
        process::exit(1);
    });

    // Parse it.
    parse(&src).unwrap_or_else(|e| {
        output::error(&format!("failed to parse `{}`: {}", path, e));
        process::exit(1);
    })
}

/// Return the platform-appropriate default shell.
///
/// Uses `sh` on Unix-like systems and `cmd` on Windows.
fn default_shell() -> String {
    if cfg!(windows) {
        "cmd".to_string()
    } else {
        "sh".to_string()
    }
}

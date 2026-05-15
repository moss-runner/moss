/// Shell completion generator for Moss.
///
/// Generates completion scripts for bash, zsh, fish, and PowerShell via
/// `clap_complete`.  Users install completions once and then get tab-
/// completion for `moss run <TAB>`, `moss list`, etc.
///
/// # Usage
///
/// ```bash
/// # bash
/// moss completions bash >> ~/.bashrc
///
/// # zsh
/// moss completions zsh >> ~/.zshrc
///
/// # fish
/// moss completions fish > ~/.config/fish/completions/moss.fish
/// ```
use std::io;

use clap::CommandFactory;
use clap_complete::{generate, Shell};

use crate::cli::Cli;

/// Write the completion script for `shell` to stdout.
pub fn generate_completions(shell: Shell) {
    let mut cmd = Cli::command();
    // `generate` writes directly to the provided writer.
    generate(shell, &mut cmd, "moss", &mut io::stdout());
}

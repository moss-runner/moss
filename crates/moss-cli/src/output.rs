/// Terminal output helpers for Moss.
///
/// All user-facing messages go through this module so that:
///   - Colors are consistent across the entire CLI.
///   - Disabling color (e.g. `NO_COLOR=1`) works from one place.
///   - The rest of the code stays free of formatting noise.
use colored::Colorize;

// ── Prefixes ──────────────────────────────────────────────────────────────────

/// Print a success line: `  ✓ <msg>` in green.
pub fn success(msg: &str) {
    println!("{} {}", "  ✓".green().bold(), msg);
}

/// Print an info line: `  · <msg>` in cyan.
pub fn info(msg: &str) {
    println!("{} {}", "  ·".cyan(), msg);
}

/// Print a warning line: `  ⚠ <msg>` in yellow.
pub fn warn(msg: &str) {
    eprintln!("{} {}", "  ⚠".yellow().bold(), msg);
}

/// Print an error line: `  ✗ <msg>` in red.
pub fn error(msg: &str) {
    eprintln!("{} {}", "  ✗".red().bold(), msg);
}

/// Print a task header banner: `  ▶ running task build` in bold.
pub fn task_start(name: &str) {
    println!(
        "\n{} {}\n",
        "  ▶".bold(),
        format!("running task `{}`", name).bold()
    );
}

/// Print a task completion line with elapsed time.
pub fn task_done(name: &str, elapsed: std::time::Duration) {
    let secs = elapsed.as_secs_f64();
    println!(
        "\n{} {} {}",
        "  ✓".green().bold(),
        format!("task `{}` finished", name).green(),
        format!("({:.2}s)", secs).dimmed(),
    );
}

/// Print a task failure line.
pub fn task_failed(name: &str) {
    eprintln!(
        "\n{} {}",
        "  ✗".red().bold(),
        format!("task `{}` failed", name).red().bold(),
    );
}

/// Print the Moss version banner on startup.
pub fn version_banner() {
    println!(
        "{} {}",
        "moss".green().bold(),
        env!("CARGO_PKG_VERSION").dimmed(),
    );
}

// ── Task list ─────────────────────────────────────────────────────────────────

/// One entry in the `moss list` output.
pub struct TaskEntry<'a> {
    pub name: &'a str,
    pub description: Option<&'a str>,
    pub flags: Vec<&'static str>,
}

/// Print a formatted task list to stdout.
///
/// ```text
///   build       Build the project in release mode
///   test   dep  Run all tests                      deps=[build]
///   dev    par  Start dev servers
/// ```
pub fn task_list(tasks: &[TaskEntry<'_>]) {
    if tasks.is_empty() {
        info("no tasks defined in Mossfile");
        return;
    }

    // Calculate the longest task name for alignment.
    let name_width = tasks.iter().map(|t| t.name.len()).max().unwrap_or(0);

    println!(); // blank line before the list
    for entry in tasks {
        // Build the flag column (max 3 chars each, space-separated).
        let flag_col: String = entry
            .flags
            .iter()
            .map(|f| format!("{:3}", f))
            .collect::<Vec<_>>()
            .join(" ");

        let desc = entry.description.unwrap_or("");

        println!(
            "  {:<width$}  {}  {}",
            entry.name.bold(),
            flag_col.cyan(),
            desc.dimmed(),
            width = name_width,
        );
    }
    println!(); // blank line after the list
}

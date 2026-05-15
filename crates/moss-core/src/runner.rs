/// Main task executor for Moss.
///
/// [`Runner`] is the central piece of `moss-core`.  Given a parsed
/// [`Mossfile`] and a target task name, it:
///
/// 1. Resolves the execution order via [`DependencyGraph`].
/// 2. Runs prerequisite tasks sequentially.
/// 3. Executes the target task — either sequentially or in parallel,
///    depending on the `parallel` flag.
/// 4. If the task has a `watch` glob, enters a watch loop and re-runs
///    the task on every relevant file-system change.
///
/// # Shell
///
/// Commands are executed via the configured shell (`sh -c <cmd>` by default).
/// The shell can be overridden with the `shell` setting in the Mossfile or
/// via [`RunnerConfig`].
use std::{collections::HashMap, process::Stdio};

use tokio::{
    io::{AsyncBufReadExt, BufReader},
    process::Command,
};

use moss_parser::{Mossfile, Task};

use crate::{
    error::RunError, graph::DependencyGraph, parallel::run_parallel, watcher::FileWatcher,
};

// ── RunnerConfig ──────────────────────────────────────────────────────────────

/// Configuration that controls how the [`Runner`] behaves.
#[derive(Debug, Clone)]
pub struct RunnerConfig {
    /// Shell binary used to execute commands (default: `"sh"`).
    pub shell: String,

    /// If `true`, print each command before running it.
    pub echo_commands: bool,
}

impl Default for RunnerConfig {
    fn default() -> Self {
        Self {
            shell: "sh".to_string(),
            echo_commands: false,
        }
    }
}

// ── Runner ────────────────────────────────────────────────────────────────────

/// Executes tasks defined in a [`Mossfile`].
pub struct Runner<'mf> {
    mossfile: &'mf Mossfile,
    config: RunnerConfig,
}

impl<'mf> Runner<'mf> {
    /// Create a new runner with default configuration.
    pub fn new(mossfile: &'mf Mossfile) -> Self {
        // Prefer the shell declared in the Mossfile; fall back to the default.
        let shell = mossfile
            .settings
            .shell
            .clone()
            .unwrap_or_else(|| "sh".to_string());

        Self {
            mossfile,
            config: RunnerConfig {
                shell,
                ..Default::default()
            },
        }
    }

    /// Override the default [`RunnerConfig`].
    pub fn with_config(mut self, config: RunnerConfig) -> Self {
        self.config = config;
        self
    }

    // ── Public entry point ────────────────────────────────────────────────────

    /// Run `target_task`, first running any prerequisites in dependency order.
    ///
    /// If the task has `watch = "…"`, this function loops indefinitely,
    /// re-running the task on every matching file-system change.
    ///
    /// # Errors
    ///
    /// Returns the first [`RunError`] encountered during execution.
    pub async fn run(&self, target_task: &str, args: &[String]) -> Result<(), RunError> {
        let graph = DependencyGraph::build(self.mossfile);
        let order = graph.execution_order(target_task)?;

        // Run all prerequisite tasks (everything except the last entry).
        for &name in order.iter().take(order.len().saturating_sub(1)) {
            let task = self.find_task(name)?;
            self.execute_task(task, &[]).await?;
        }

        // Run the target task itself.
        let target = self.find_task(target_task)?;

        match &target.flags.watch {
            None => {
                // Single run.
                self.execute_task(target, args).await?;
            }
            Some(watch_cfg) => {
                // Watch loop: run once, then re-run on every file change.
                self.execute_task(target, args).await?;

                let mut watcher = FileWatcher::new(&watch_cfg.glob, target_task)?;

                loop {
                    println!("\n  watching {} for changes…\n", watch_cfg.glob);
                    watcher.wait_for_change().await;
                    println!("\n  change detected — restarting `{}`…\n", target_task);
                    self.execute_task(target, args).await?;
                }
            }
        }

        Ok(())
    }

    // ── Task execution ────────────────────────────────────────────────────────

    /// Execute a single task, respecting its `parallel` flag and `args`.
    async fn execute_task(&self, task: &Task, args: &[String]) -> Result<(), RunError> {
        if self.config.echo_commands {
            println!("  running task `{}`", task.name);
        }

        // Substitute `{{arg}}` placeholders with the provided argument values.
        let commands = self.substitute_args(task, args);

        if task.flags.parallel {
            run_parallel(&task.name, &commands, &self.config.shell).await
        } else {
            self.run_sequential(&task.name, &commands).await
        }
    }

    /// Run `commands` one at a time, stopping at the first failure.
    async fn run_sequential(&self, task_name: &str, commands: &[String]) -> Result<(), RunError> {
        for cmd in commands {
            self.run_command(task_name, cmd).await?;
        }
        Ok(())
    }

    /// Spawn a single shell command, stream its output, and wait for it to exit.
    async fn run_command(&self, task_name: &str, cmd: &str) -> Result<(), RunError> {
        if self.config.echo_commands {
            println!("  $ {}", cmd);
        }

        let mut child = Command::new(&self.config.shell)
            .args(["-c", cmd])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| RunError::SpawnFailed {
                task: task_name.to_string(),
                cmd: cmd.to_string(),
                source: e,
            })?;

        // Stream stdout.
        if let Some(stdout) = child.stdout.take() {
            let mut lines = BufReader::new(stdout).lines();
            tokio::spawn(async move {
                while let Ok(Some(line)) = lines.next_line().await {
                    println!("{}", line);
                }
            });
        }

        // Stream stderr.
        if let Some(stderr) = child.stderr.take() {
            let mut lines = BufReader::new(stderr).lines();
            tokio::spawn(async move {
                while let Ok(Some(line)) = lines.next_line().await {
                    eprintln!("{}", line);
                }
            });
        }

        let status = child.wait().await.map_err(|e| RunError::Io {
            task: task_name.to_string(),
            source: e,
        })?;

        if status.success() {
            return Ok(());
        }

        let code = status.code().unwrap_or(-1);

        #[cfg(unix)]
        if status.code().is_none() {
            return Err(RunError::CommandKilled {
                task: task_name.to_string(),
                cmd: cmd.to_string(),
            });
        }

        Err(RunError::CommandFailed {
            task: task_name.to_string(),
            cmd: cmd.to_string(),
            code,
        })
    }

    // ── Argument substitution ─────────────────────────────────────────────────

    /// Replace `{{name}}` placeholders in every command with the corresponding
    /// positional argument value.
    ///
    /// If fewer arguments are provided than declared, the placeholder is left
    /// as-is (the parser already validated that placeholders are declared;
    /// missing runtime values produce visible `{{name}}` in the command).
    fn substitute_args(&self, task: &Task, args: &[String]) -> Vec<String> {
        // Build a name → value map from declared arg names and provided values.
        let values: HashMap<&str, &str> = task
            .flags
            .args
            .iter()
            .zip(args.iter())
            .map(|(name, val)| (name.as_str(), val.as_str()))
            .collect();

        task.commands
            .iter()
            .map(|cmd| {
                let mut result = cmd.raw.clone();
                for (name, value) in &values {
                    let placeholder = format!("{{{{{}}}}}", name);
                    result = result.replace(&placeholder, value);
                }
                result
            })
            .collect()
    }

    // ── Helpers ───────────────────────────────────────────────────────────────

    /// Look up a task by name, returning [`RunError::TaskNotFound`] if absent.
    fn find_task(&self, name: &str) -> Result<&Task, RunError> {
        self.mossfile
            .tasks
            .iter()
            .find(|t| t.name == name)
            .ok_or_else(|| RunError::TaskNotFound(name.to_string()))
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use moss_parser::parse;

    /// Helper: parse src and run `task_name` synchronously.
    async fn run(src: &str, task_name: &str) -> Result<(), RunError> {
        let mf = parse(src).unwrap();
        Runner::new(&mf).run(task_name, &[]).await
    }

    #[tokio::test]
    async fn test_simple_echo_task() {
        let src = "task greet:\n  echo hello\n";
        assert!(run(src, "greet").await.is_ok());
    }

    #[tokio::test]
    async fn test_unknown_task_error() {
        let src = "task build:\n  cargo build\n";
        assert!(matches!(
            run(src, "nonexistent").await,
            Err(RunError::TaskNotFound(_))
        ));
    }

    #[tokio::test]
    async fn test_failing_command() {
        // `exit 1` is a valid shell command that exits non-zero.
        let src = "task fail:\n  exit 1\n";
        assert!(matches!(
            run(src, "fail").await,
            Err(RunError::CommandFailed { .. })
        ));
    }

    #[tokio::test]
    async fn test_deps_run_before_target() {
        // Both tasks just echo; we verify no error means both ran.
        let src = concat!(
            "task setup:\n  echo setup\n",
            "task build deps=[setup]:\n  echo build\n",
        );
        assert!(run(src, "build").await.is_ok());
    }

    #[tokio::test]
    async fn test_arg_substitution() {
        let src = "task greet args=[name]:\n  echo hello {{name}}\n";
        let mf = parse(src).unwrap();
        let runner = Runner::new(&mf);
        // Run with a provided argument.
        assert!(runner.run("greet", &["world".to_string()]).await.is_ok());
    }

    #[tokio::test]
    async fn test_parallel_commands() {
        let src = "task ping parallel:\n  echo one\n  echo two\n";
        assert!(run(src, "ping").await.is_ok());
    }
}

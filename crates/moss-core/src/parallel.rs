/// Parallel command executor for Moss.
///
/// When a task is declared with the `parallel` flag, every command in its
/// body is spawned concurrently.  If **any** command exits with a non-zero
/// code, all sibling processes are killed immediately (fail-fast semantics).
///
/// # Design
///
/// Each command gets its own [`tokio::task`].  A shared
/// [`tokio::sync::watch`] channel carries a "kill" signal; every task
/// monitors the channel and terminates its child process as soon as the
/// signal fires.
use std::process::Stdio;

use tokio::{
    io::{AsyncBufReadExt, BufReader},
    process::Command,
    sync::watch,
    task::JoinSet,
};

use crate::error::RunError;

// ── Public entry point ────────────────────────────────────────────────────────

/// Run every command in `commands` concurrently under `shell`.
///
/// Streams each line of stdout/stderr to stdout, prefixed with the command
/// index so the user can tell which process produced which output.
///
/// Returns `Ok(())` only if **all** commands exit with code 0.
/// Returns [`RunError::ParallelFailure`] (with a count) if any fail.
///
/// # Arguments
///
/// * `task_name`  – used in error messages and output prefixes.
/// * `commands`   – raw command strings to execute.
/// * `shell`      – shell binary to use (e.g. `"sh"`, `"bash"`).
pub async fn run_parallel(
    task_name: &str,
    commands: &[String],
    shell: &str,
) -> Result<(), RunError> {
    if commands.is_empty() {
        return Ok(());
    }

    // The kill channel: sending `true` tells all workers to stop.
    let (kill_tx, kill_rx) = watch::channel(false);

    let mut join_set: JoinSet<Result<(), RunError>> = JoinSet::new();

    for (idx, cmd) in commands.iter().enumerate() {
        let cmd = cmd.clone();
        let shell = shell.to_string();
        let task_name = task_name.to_string();
        let kill_rx = kill_rx.clone();

        join_set
            .spawn(async move { run_one_parallel(idx, &task_name, &cmd, &shell, kill_rx).await });
    }

    // Collect results; if any worker failed, send the kill signal.
    let mut failures = 0usize;

    while let Some(result) = join_set.join_next().await {
        match result {
            Ok(Ok(())) => {}
            Ok(Err(_)) | Err(_) => {
                failures += 1;
                // Signal all remaining workers to abort.
                let _ = kill_tx.send(true);
            }
        }
    }

    if failures > 0 {
        Err(RunError::ParallelFailure {
            task: task_name.to_string(),
            count: failures,
        })
    } else {
        Ok(())
    }
}

// ── Worker ────────────────────────────────────────────────────────────────────

/// Spawn and supervise a single command within a parallel group.
///
/// Monitors `kill_rx`; if it becomes `true`, the child process is killed
/// before this function returns.
async fn run_one_parallel(
    idx: usize,
    task_name: &str,
    cmd: &str,
    shell: &str,
    mut kill_rx: watch::Receiver<bool>,
) -> Result<(), RunError> {
    let mut child = Command::new(shell)
        .args(["-c", cmd])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| RunError::SpawnFailed {
            task: task_name.to_string(),
            cmd: cmd.to_string(),
            source: e,
        })?;

    let stdout = child.stdout.take().map(|s| BufReader::new(s).lines());
    let stderr = child.stderr.take().map(|s| BufReader::new(s).lines());

    // Print output with a `[idx] ` prefix so parallel streams are readable.
    let prefix = format!("[{}]", idx);

    // Stream stdout in a background task.
    if let Some(mut lines) = stdout {
        let pfx = prefix.clone();
        tokio::spawn(async move {
            while let Ok(Some(line)) = lines.next_line().await {
                println!("{} {}", pfx, line);
            }
        });
    }

    // Stream stderr in a background task.
    if let Some(mut lines) = stderr {
        let pfx = prefix.clone();
        tokio::spawn(async move {
            while let Ok(Some(line)) = lines.next_line().await {
                eprintln!("{} {}", pfx, line);
            }
        });
    }

    // Wait for either the child to finish or a kill signal.
    loop {
        tokio::select! {
            // Child process finished.
            status = child.wait() => {
                let status = status.map_err(|e| RunError::Io {
                    task: task_name.to_string(),
                    source: e,
                })?;

                if status.success() {
                    return Ok(());
                }

                let code = status.code().unwrap_or(-1);
                return Err(RunError::CommandFailed {
                    task: task_name.to_string(),
                    cmd: cmd.to_string(),
                    code,
                });
            }

            // Kill signal received from a failing sibling.
            _ = kill_rx.changed() => {
                if *kill_rx.borrow() {
                    // Best-effort kill; ignore errors (process may have already exited).
                    let _ = child.kill().await;
                    return Err(RunError::CommandKilled {
                        task: task_name.to_string(),
                        cmd: cmd.to_string(),
                    });
                }
            }
        }
    }
}

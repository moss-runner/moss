/// Runtime error types for moss-core.
///
/// Separates execution errors from parse errors so callers can handle
/// each category independently.
use thiserror::Error;

/// All errors that can occur during task execution.
#[derive(Debug, Error)]
pub enum RunError {
    /// A required task name was not found in the Mossfile.
    #[error("task `{0}` not found")]
    TaskNotFound(String),

    /// A shell command failed with a non-zero exit code.
    #[error("command `{cmd}` in task `{task}` exited with code {code}")]
    CommandFailed {
        task: String,
        cmd: String,
        code: i32,
    },

    /// A command was killed by a signal (Unix only).
    #[error("command `{cmd}` in task `{task}` was killed by a signal")]
    CommandKilled { task: String, cmd: String },

    /// The OS refused to spawn the process (e.g. binary not found).
    #[error("failed to spawn command `{cmd}` in task `{task}`: {source}")]
    SpawnFailed {
        task: String,
        cmd: String,
        #[source]
        source: std::io::Error,
    },

    /// An I/O error occurred while reading process output.
    #[error("I/O error while running task `{task}`: {source}")]
    Io {
        task: String,
        #[source]
        source: std::io::Error,
    },

    /// The file watcher failed to initialise.
    #[error("watcher error for task `{task}`: {message}")]
    WatcherInit { task: String, message: String },

    /// One or more parallel tasks failed (aggregated).
    #[error("{count} parallel task(s) failed in `{task}`")]
    ParallelFailure { task: String, count: usize },
}

/// Ready-pattern detector for long-running tasks.
///
/// When a task declares `ready = "some string"`, dependent tasks should not
/// start until the process has printed that string to stdout or stderr.
///
/// This module provides [`ReadyDetector`], which wraps an async reader and
/// signals a [`tokio::sync::watch`] channel the moment the pattern appears.
use tokio::{
    io::{AsyncBufReadExt, BufReader},
    process::ChildStdout,
    sync::watch,
};

// ── ReadyDetector ─────────────────────────────────────────────────────────────

/// Watches a child process's stdout for a trigger string.
///
/// # Example
///
/// ```rust,no_run
/// use tokio::process::Command;
/// use std::process::Stdio;
/// use moss_core::ready::ReadyDetector;
///
/// #[tokio::main]
/// async fn main() {
///     // Kita harus benar-benar mendefinisikan 'child' agar doc-test lulus
///     let mut child = Command::new("echo")
///         .arg("Listening on 8080")
///         .stdout(Stdio::piped())
///         .spawn()
///         .unwrap();
///
///     let stdout = child.stdout.take().unwrap();
///     let mut detector = ReadyDetector::new(stdout, "Listening on".to_string());
///     let mut rx = detector.ready_rx();
///
///     tokio::spawn(async move {
///         detector.run(|line| println!("{}", line)).await
///     });
///
///     rx.changed().await.unwrap();
///     println!("Server is ready!");
/// }
/// ```
pub struct ReadyDetector {
    stdout: ChildStdout,
    /// The substring to look for in each output line.
    pattern: String,
    /// Sender half of the ready signal channel.
    tx: watch::Sender<bool>,
    /// Receiver half — cloned and handed to callers.
    rx: watch::Receiver<bool>,
}

impl ReadyDetector {
    /// Create a new detector that watches `stdout` for `pattern`.
    pub fn new(stdout: ChildStdout, pattern: String) -> Self {
        let (tx, rx) = watch::channel(false);
        Self {
            stdout,
            pattern,
            tx,
            rx,
        }
    }

    /// Return a receiver that becomes `true` once the pattern is seen.
    ///
    /// Clone this before calling [`run`](Self::run) since `run` consumes `self`.
    pub fn ready_rx(&self) -> watch::Receiver<bool> {
        self.rx.clone()
    }

    /// Drive the detector to completion.
    ///
    /// Reads lines from stdout, forwarding each to the provided `line_sink`
    /// callback (so the caller can still display output), and sends `true`
    /// on the watch channel the first time the ready pattern is matched.
    ///
    /// Returns when the child closes its stdout (EOF).
    pub async fn run<F>(self, mut line_sink: F)
    where
        F: FnMut(String) + Send + 'static,
    {
        let mut reader = BufReader::new(self.stdout).lines();
        let mut signalled = false;

        while let Ok(Some(line)) = reader.next_line().await {
            // Forward the line to whatever is displaying output.
            line_sink(line.clone());

            // Signal readiness once — subsequent matches are ignored.
            if !signalled && line.contains(&self.pattern) {
                // Errors here mean all receivers were dropped; safe to ignore.
                let _ = self.tx.send(true);
                signalled = true;
            }
        }

        // If the process exited without ever matching, signal anyway so that
        // dependent tasks are not blocked forever.
        if !signalled {
            let _ = self.tx.send(true);
        }
    }
}

// ── Utility ───────────────────────────────────────────────────────────────────

/// Wait until `rx` becomes `true`, or return immediately if it already is.
pub async fn wait_until_ready(mut rx: watch::Receiver<bool>) {
    // If the value is already true (e.g. task finished instantly), return now.
    if *rx.borrow() {
        return;
    }
    // Otherwise wait for the next change.
    let _ = rx.changed().await;
}

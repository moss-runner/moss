//! Built-in file watcher for Moss.
//!
//! When a task declares `watch = "src/**/*.rs"`, Moss re-runs the task
//! automatically whenever a matching file changes.
//!
//! This module wraps the [`notify`] crate behind a clean async interface
//! so the runner never has to touch notify directly.
use std::{
    path::Path,
    sync::{Arc, Mutex},
    time::Duration,
};

use notify::{Config, Event, RecommendedWatcher, RecursiveMode, Watcher};
use tokio::sync::mpsc;

use crate::error::RunError;

// ── FileWatcher ───────────────────────────────────────────────────────────────

/// Watches a glob pattern and sends a signal whenever a matching file changes.
///
/// # Example
///
/// ```rust,no_run
/// # use moss_core::watcher::FileWatcher;
/// # #[tokio::main] async fn main() {
/// let mut watcher = FileWatcher::new("src/**/*.rs", "my-task").unwrap();
///
/// loop {
///     watcher.wait_for_change().await;
///     println!("File changed — restarting task…");
///     // re-run your task here
/// }
/// # }
/// ```
pub struct FileWatcher {
    /// Receiver that yields `()` whenever a relevant file event fires.
    rx: mpsc::Receiver<()>,
    /// Keep the notify watcher alive for as long as this struct lives.
    _watcher: RecommendedWatcher,
}

impl FileWatcher {
    /// Create a new watcher for the given glob `pattern`.
    ///
    /// `task_name` is used only for error messages.
    ///
    /// # Errors
    ///
    /// Returns [`RunError::WatcherInit`] if notify cannot initialise the
    /// OS-level file-system watcher.
    pub fn new(pattern: &str, task_name: &str) -> Result<Self, RunError> {
        let (tx, rx) = mpsc::channel::<()>(8);

        // Clone the pattern into the closure so it outlives this function.
        let pattern = pattern.to_string();
        let task_name = task_name.to_string();

        // notify requires a synchronous callback; we bridge to async with the
        // channel above and a std Mutex to share the sender.
        let tx_arc = Arc::new(Mutex::new(tx));

        let tx_clone = Arc::clone(&tx_arc);
        let pat_clone = pattern.clone();

        let mut watcher = notify::recommended_watcher(move |res: notify::Result<Event>| {
            let Ok(event) = res else { return };

            // Only care about modification / creation / removal events.
            if !matches!(
                event.kind,
                notify::EventKind::Modify(_)
                    | notify::EventKind::Create(_)
                    | notify::EventKind::Remove(_)
            ) {
                return;
            }

            // Filter by glob pattern.
            let matches_glob = event.paths.iter().any(|p| glob_matches(&pat_clone, p));

            if matches_glob {
                // Best-effort send; if the channel is full we skip this event.
                if let Ok(tx) = tx_clone.lock() {
                    let _ = tx.try_send(());
                }
            }
        })
        .map_err(|e| RunError::WatcherInit {
            task: task_name.clone(),
            message: e.to_string(),
        })?;

        // Determine the root directory to watch from the glob pattern.
        // E.g. "src/**/*.rs" → watch "src/" recursively.
        let watch_root = glob_root(&pattern);

        watcher
            .watch(Path::new(&watch_root), RecursiveMode::Recursive)
            .map_err(|e| RunError::WatcherInit {
                task: task_name.clone(),
                message: format!("cannot watch `{}`: {}", watch_root, e),
            })?;

        // Apply a debounce-like config to avoid floods of events.
        watcher
            .configure(Config::default().with_poll_interval(Duration::from_millis(300)))
            .ok(); // configure errors are non-fatal

        Ok(Self {
            rx,
            _watcher: watcher,
        })
    }

    /// Suspend until at least one relevant file-system change is detected.
    ///
    /// Drains any additional events that arrive within a short debounce window
    /// so that saving multiple files at once triggers only one re-run.
    pub async fn wait_for_change(&mut self) {
        // Block until the first event.
        self.rx.recv().await;

        // Drain extra events that pile up within the debounce window (100 ms).
        let debounce = Duration::from_millis(100);
        while let Ok(Some(())) = tokio::time::timeout(debounce, self.rx.recv()).await {
            // Another event arrived — keep draining until the window expires.
        }
    }
}

// ── Helpers ───────────────────────────────────────────────────────────────────

/// Test whether `path` matches the given glob `pattern`.
fn glob_matches(pattern: &str, path: &Path) -> bool {
    let Ok(pat) = glob::Pattern::new(pattern) else {
        return false;
    };
    pat.matches_path(path)
}

/// Extract the non-wildcard root directory from a glob pattern.
///
/// `"src/**/*.rs"` → `"src"`, `"*.rs"` → `"."`, `"a/b/c.rs"` → `"a/b"`.
fn glob_root(pattern: &str) -> String {
    // Walk the pattern left-to-right; stop at the first wildcard segment.
    let mut root = String::from(".");
    for segment in pattern.split('/') {
        if segment.contains('*') || segment.contains('?') || segment.contains('[') {
            break;
        }
        if root == "." {
            root = segment.to_string();
        } else {
            root.push('/');
            root.push_str(segment);
        }
    }
    root
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_glob_root_with_wildcard() {
        assert_eq!(glob_root("src/**/*.rs"), "src");
    }

    #[test]
    fn test_glob_root_no_wildcard() {
        assert_eq!(glob_root("src/main.rs"), "src/main.rs");
    }

    #[test]
    fn test_glob_root_bare_wildcard() {
        assert_eq!(glob_root("*.rs"), ".");
    }

    #[test]
    fn test_glob_matches_rs_file() {
        let path = PathBuf::from("src/main.rs");
        assert!(glob_matches("src/**/*.rs", &path));
    }

    #[test]
    fn test_glob_no_match_different_ext() {
        let path = PathBuf::from("src/main.toml");
        assert!(!glob_matches("src/**/*.rs", &path));
    }
}

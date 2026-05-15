/// `moss-core` — public API
///
/// The execution engine for the Moss task runner.
///
/// # Quick start
///
/// ```rust,no_run
/// use moss_core::runner::Runner;
/// use moss_parser::parse;
///
/// #[tokio::main]
/// async fn main() {
///     let src = std::fs::read_to_string("Mossfile").unwrap();
///     let mossfile = parse(&src).unwrap();
///
///     Runner::new(&mossfile)
///         .run("build", &[])
///         .await
///         .unwrap();
/// }
/// ```
pub mod error;
pub mod graph;
pub mod parallel;
pub mod ready;
pub mod runner;
pub mod watcher;

// Re-export the most commonly used types at the crate root.
pub use error::RunError;
pub use runner::{Runner, RunnerConfig};

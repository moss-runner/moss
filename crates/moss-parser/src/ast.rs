// AST node definitions for a parsed Mossfile.
//
// The hierarchy is:
//   Mossfile
//   └── Vec<Task>
//       ├── name, description, flags
//       └── Vec<Command>
//
// Every node stores a [`Span`] so that downstream tools (error messages,
// language servers) can point back to the original source text.

// ── Span ─────────────────────────────────────────────────────────────────────

// A half-open byte range `[start, end)` inside the source string.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Span {
    pub start: usize,
    pub end: usize,
}

impl Span {
    pub fn new(start: usize, end: usize) -> Self {
        Self { start, end }
    }

    // Merge two spans into one that covers both.
    pub fn merge(self, other: Span) -> Span {
        Span {
            start: self.start.min(other.start),
            end: self.end.max(other.end),
        }
    }
}

// ── Top-level Mossfile ────────────────────────────────────────────────────────

// The root node produced by a successful parse.
#[derive(Debug, Clone, PartialEq)]
pub struct Mossfile {
    // Global settings declared at the top of the file (e.g. `project = "…"`).
    pub settings: Settings,

    // All task declarations, in source order.
    pub tasks: Vec<Task>,
}

// Key/value settings that appear before the first `task` declaration.
//
// ```text
// project = "my-app"
// shell   = "sh"
// ```
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Settings {
    // Optional project name (`project = "…"`).
    pub project: Option<String>,

    // Shell used to run commands.
    // Defaults to `"sh"` on Unix and `"cmd"` on Windows when not set.
    pub shell: Option<String>,
}

// ── Task ──────────────────────────────────────────────────────────────────────

// A single `task` declaration.
//
// ```text
// task build:
//   cargo build --release
//
// task test deps=[build]:
//   desc = "Run all tests"
//   cargo test
//
// task dev parallel:
//   cargo watch -x run
//   npm run dev
// ```
#[derive(Debug, Clone, PartialEq)]
pub struct Task {
    // Task name as written in the source.
    pub name: String,

    // Optional human-readable description (`desc = "…"` inside the body).
    pub description: Option<String>,

    // Behaviour flags parsed from the task header line.
    pub flags: TaskFlags,

    // The shell commands that make up the task body.
    pub commands: Vec<Command>,

    // Source location of the entire task block.
    pub span: Span,
}

// ── TaskFlags ─────────────────────────────────────────────────────────────────

// Inline flags that appear on the `task` header line.
//
// ```text
// task dev parallel:                  <- parallel flag
// task serve watch="src/**/*.rs":     <- watch flag with glob
// task test deps=[build, lint]:       <- dependency list
// task deploy args=[env]:             <- declared positional arguments
// ```
#[derive(Debug, Clone, PartialEq, Default)]
pub struct TaskFlags {
    // `parallel` — run every command in the body concurrently.
    // If any command exits non-zero, all sibling processes are killed.
    pub parallel: bool,

    // `watch="<glob>"` — re-run the task whenever a matching file changes.
    pub watch: Option<WatchConfig>,

    // `ready="<pattern>"` — mark the task as ready once its stdout contains
    // the given substring. Dependent tasks wait for this signal before starting.
    pub ready: Option<String>,

    // `deps=[task1, task2, …]` — tasks that must complete before this one runs.
    pub deps: Vec<String>,

    // `args=[name1, name2, …]` — positional arguments the task accepts.
    // Referenced inside commands as `{{name}}`.
    pub args: Vec<String>,
}

// Configuration for the built-in file watcher.
#[derive(Debug, Clone, PartialEq)]
pub struct WatchConfig {
    // Glob pattern to watch, e.g. `"src/**/*.rs"`.
    pub glob: String,
}

// ── Command ───────────────────────────────────────────────────────────────────

// A single shell command inside a task body.
//
// ```text
// task build:
//   cargo build --release    <- this is a Command
// ```
#[derive(Debug, Clone, PartialEq)]
pub struct Command {
    // The raw command string; may contain `{{arg}}` placeholders.
    pub raw: String,

    // Source location of this command line.
    pub span: Span,
}

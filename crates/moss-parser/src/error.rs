/// Parse error types for Mossfile.
///
/// Every error variant carries a [`Location`] so callers can show the user
/// exactly which line and column caused the problem.
use thiserror::Error;

// ── Location ──────────────────────────────────────────────────────────────────

/// Human-readable source position (1-indexed line and column).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Location {
    pub line: usize,
    pub column: usize,
}

impl std::fmt::Display for Location {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "line {}, column {}", self.line, self.column)
    }
}

/// Convert a byte offset within `src` to a [`Location`].
pub fn offset_to_location(src: &str, offset: usize) -> Location {
    let safe_offset = offset.min(src.len());
    let before = &src[..safe_offset];
    let line = before.chars().filter(|&c| c == '\n').count() + 1;
    let column = before
        .rfind('\n')
        .map(|p| safe_offset - p)
        .unwrap_or(safe_offset + 1);
    Location { line, column }
}

// ── ParseError ────────────────────────────────────────────────────────────────

/// All errors that the lexer or parser can produce.
#[derive(Debug, Error, PartialEq)]
pub enum ParseError {
    // ── Lexer errors ──────────────────────────────────────────────────────────
    /// A string literal was opened with `"` but never closed.
    #[error("unterminated string literal at {location}")]
    UnterminatedString { location: Location },

    /// A character appeared where no valid token can start.
    #[error("unexpected character {ch:?} at {location}")]
    UnexpectedChar { ch: char, location: Location },

    // ── Parser errors ─────────────────────────────────────────────────────────
    /// The `task` keyword was followed by something other than an identifier.
    #[error("expected task name after `task` at {location}")]
    ExpectedTaskName { location: Location },

    /// A task header was not terminated with `:`.
    #[error("expected `:` at end of task header at {location}")]
    ExpectedColon { location: Location },

    /// A `deps=[…]` or `args=[…]` list was not closed with `]`.
    #[error("unclosed `[` in task header at {location}")]
    UnclosedBracket { location: Location },

    /// A `watch="…"` or `ready="…"` flag was missing its `=` sign.
    #[error("expected `=` after `{flag}` at {location}")]
    ExpectedEquals { flag: String, location: Location },

    /// A flag value string was expected but something else was found.
    #[error("expected string value for flag `{flag}` at {location}")]
    ExpectedStringValue { flag: String, location: Location },

    /// A task body contained no commands and no description.
    #[error("task `{name}` has an empty body at {location}")]
    EmptyTaskBody { name: String, location: Location },

    /// A setting key was recognised but its value was invalid.
    #[error("invalid value for setting `{key}` at {location}")]
    InvalidSettingValue { key: String, location: Location },

    /// A `{{…}}` placeholder in a command references an undeclared argument.
    #[error(
        "command in task `{task}` uses undeclared argument `{arg}` at {location}\n\
         hint: add `args=[{arg}]` to the task header"
    )]
    UndeclaredArgument {
        task: String,
        arg: String,
        location: Location,
    },

    /// Two tasks share the same name.
    #[error("duplicate task name `{name}` at {location}")]
    DuplicateTask { name: String, location: Location },

    /// A `deps=[…]` list references a task that does not exist.
    #[error(
        "task `{task}` depends on unknown task `{dep}` at {location}\n\
         hint: make sure `{dep}` is declared in the same Mossfile"
    )]
    UnknownDependency {
        task: String,
        dep: String,
        location: Location,
    },

    /// Following dependency edges would form a cycle.
    #[error("dependency cycle detected: {cycle}")]
    DependencyCycle { cycle: String },
}

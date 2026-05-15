/// `moss-parser` — public API
///
/// The crate exposes a single convenience function [`parse`] and re-exports
/// all AST types so downstream crates only need to depend on `moss-parser`.
///
/// # Example
///
/// ```rust
/// use moss_parser::parse;
///
/// let src = r#"
/// project = "my-app"
///
/// task build:
///   cargo build --release
///
/// task test deps=[build]:
///   desc = "Run all tests"
///   cargo test
///
/// task dev parallel:
///   cargo watch -x run
///   npm run dev
/// "#;
///
/// let mossfile = parse(src).unwrap();
/// assert_eq!(mossfile.tasks.len(), 3);
/// assert_eq!(mossfile.tasks[0].name, "build");
/// assert!(mossfile.tasks[2].flags.parallel);
/// ```
pub mod ast;
pub mod error;
pub mod lexer;
pub mod parser;

// Re-export the most commonly used types at the crate root.
pub use ast::{Command, Mossfile, Settings, Span, Task, TaskFlags, WatchConfig};
pub use error::ParseError;

/// Parse a Mossfile source string into a [`Mossfile`] AST.
///
/// This is the primary entry point for the crate.
///
/// # Errors
///
/// Returns a [`ParseError`] if the source contains a lexical or syntax error,
/// a duplicate task name, an unknown dependency, a dependency cycle, or an
/// undeclared `{{arg}}` placeholder.
pub fn parse(src: &str) -> Result<Mossfile, ParseError> {
    let tokens = lexer::Lexer::new(src).tokenize()?;
    parser::Parser::new(tokens, src).parse()
}

/// Recursive-descent parser for Mossfile tokens.
///
/// Consumes the flat [`Vec<Token>`] produced by the lexer and builds the
/// [`Mossfile`] AST.  Validation (unknown deps, duplicate tasks, cycles) is
/// done in a second pass after the tree is fully built — see [`validate`].
///
/// # Grammar (informal)
///
/// ```text
/// mossfile   = setting* task*
///
/// setting    = ("project" | "shell") "=" string NEWLINE
///
/// task       = "task" IDENT flag* ":" NEWLINE body
///
/// flag       = "parallel"
///            | "watch"  "=" string
///            | "ready"  "=" string
///            | "deps"   "=" "[" ident_list "]"
///            | "args"   "=" "[" ident_list "]"
///
/// body       = (desc_line | command_line)+
///
/// desc_line  = INDENT "desc" "=" string NEWLINE
/// command_line = INDENT IDENT NEWLINE       <- the IDENT holds the raw cmd
///
/// ident_list = IDENT ("," IDENT)*
/// ```
use crate::ast::*;
use crate::error::{offset_to_location, ParseError};
use crate::lexer::{Token, TokenKind};

// ── Parser ────────────────────────────────────────────────────────────────────

pub struct Parser {
    tokens: Vec<Token>,
    /// Index of the next token to inspect.
    cursor: usize,
    /// Original source, kept for error location conversion.
    src: String,
}

impl Parser {
    pub fn new(tokens: Vec<Token>, src: impl Into<String>) -> Self {
        Self {
            tokens,
            cursor: 0,
            src: src.into(),
        }
    }

    // ── Top-level entry point ─────────────────────────────────────────────────

    /// Parse all tokens into a [`Mossfile`] and run validation.
    pub fn parse(mut self) -> Result<Mossfile, ParseError> {
        let settings = self.parse_settings()?;
        let mut tasks: Vec<Task> = Vec::new();

        while !self.is_at_end() {
            self.skip_newlines();
            if self.is_at_end() {
                break;
            }

            match self.peek_kind() {
                TokenKind::Task => tasks.push(self.parse_task()?),
                _ => {
                    // Unexpected top-level token — skip with a helpful error.
                    let span = self.peek().span;
                    let loc = offset_to_location(&self.src, span.start);
                    return Err(ParseError::ExpectedTaskName { location: loc });
                }
            }
        }

        let mossfile = Mossfile { settings, tasks };
        validate(&mossfile, &self.src)?;
        Ok(mossfile)
    }

    // ── Settings ──────────────────────────────────────────────────────────────

    /// Parse zero or more top-level `key = "value"` settings.
    fn parse_settings(&mut self) -> Result<Settings, ParseError> {
        let mut settings = Settings::default();

        loop {
            self.skip_newlines();
            match self.peek_kind() {
                TokenKind::Project => {
                    self.advance();
                    self.expect_equals("project")?;
                    settings.project = Some(self.expect_string("project")?);
                    self.skip_newlines();
                }
                TokenKind::Shell => {
                    self.advance();
                    self.expect_equals("shell")?;
                    settings.shell = Some(self.expect_string("shell")?);
                    self.skip_newlines();
                }
                // Anything else ends the settings section.
                _ => break,
            }
        }

        Ok(settings)
    }

    // ── Task ──────────────────────────────────────────────────────────────────

    /// Parse one `task <name> [flags…] : <body>` block.
    fn parse_task(&mut self) -> Result<Task, ParseError> {
        let task_start = self.peek().span.start;

        // Consume `task`.
        self.advance();

        // Task name must be a bare identifier.
        let name = match self.peek_kind().clone() {
            TokenKind::Ident(n) => {
                self.advance();
                n
            }
            _ => {
                let loc = offset_to_location(&self.src, self.peek().span.start);
                return Err(ParseError::ExpectedTaskName { location: loc });
            }
        };

        // Parse inline flags until we hit `:`.
        let flags = self.parse_flags(&name)?;

        // Expect the closing `:`.
        if self.peek_kind() != &TokenKind::Colon {
            let loc = offset_to_location(&self.src, self.peek().span.start);
            return Err(ParseError::ExpectedColon { location: loc });
        }
        self.advance(); // consume `:`
        self.skip_newlines();

        // Parse the indented body.
        let (description, commands, body_end) = self.parse_body(&name)?;

        if commands.is_empty() && description.is_none() {
            let loc = offset_to_location(&self.src, task_start);
            return Err(ParseError::EmptyTaskBody {
                name,
                location: loc,
            });
        }

        Ok(Task {
            name,
            description,
            flags,
            commands,
            span: Span::new(task_start, body_end),
        })
    }

    // ── Flags ─────────────────────────────────────────────────────────────────

    /// Parse all flags on the task header line (everything between name and `:`).
    fn parse_flags(&mut self, task_name: &str) -> Result<TaskFlags, ParseError> {
        let mut flags = TaskFlags::default();

        loop {
            match self.peek_kind().clone() {
                TokenKind::Colon | TokenKind::Newline | TokenKind::Eof => break,

                TokenKind::Parallel => {
                    self.advance();
                    flags.parallel = true;
                }

                TokenKind::Watch => {
                    self.advance();
                    self.expect_equals("watch")?;
                    let glob = self.expect_string("watch")?;
                    flags.watch = Some(WatchConfig { glob });
                }

                TokenKind::Ready => {
                    self.advance();
                    self.expect_equals("ready")?;
                    let pattern = self.expect_string("ready")?;
                    flags.ready = Some(pattern);
                }

                TokenKind::Deps => {
                    self.advance();
                    self.expect_equals("deps")?;
                    flags.deps = self.parse_ident_list()?;
                }

                TokenKind::Args => {
                    self.advance();
                    self.expect_equals("args")?;
                    flags.args = self.parse_ident_list()?;
                }

                _ => {
                    // Unknown flag — emit a helpful error pointing at the token.
                    let loc = offset_to_location(&self.src, self.peek().span.start);
                    return Err(ParseError::ExpectedColon { location: loc });
                }
            }
        }

        let _ = task_name; // reserved for future use in diagnostics
        Ok(flags)
    }

    // ── Body ──────────────────────────────────────────────────────────────────

    /// Parse the indented body of a task.
    ///
    /// Returns `(description, commands, end_byte_offset)`.
    fn parse_body(
        &mut self,
        task_name: &str,
    ) -> Result<(Option<String>, Vec<Command>, usize), ParseError> {
        let mut description: Option<String> = None;
        let mut commands: Vec<Command> = Vec::new();
        let mut end = self.peek().span.start;

        loop {
            // A body line starts with an Indent token.
            if !matches!(self.peek_kind(), TokenKind::Indent(_)) {
                break;
            }
            self.advance(); // consume Indent

            match self.peek_kind().clone() {
                // `desc = "…"` line.
                TokenKind::Desc => {
                    self.advance();
                    self.expect_equals("desc")?;
                    description = Some(self.expect_string("desc")?);
                    self.skip_newlines();
                }

                // A raw command — the lexer stored it as an Ident.
                TokenKind::Ident(raw) => {
                    let cmd_span = self.peek().span;
                    self.advance();
                    end = cmd_span.end;

                    // Validate `{{arg}}` placeholders against declared args.
                    self.check_placeholders(task_name, &raw, cmd_span.start)?;

                    commands.push(Command {
                        raw,
                        span: cmd_span,
                    });
                    self.skip_newlines();
                }

                // Anything else ends the body.
                _ => break,
            }
        }

        Ok((description, commands, end))
    }

    // ── Placeholder validation ────────────────────────────────────────────────

    /// Verify every `{{name}}` placeholder in `cmd` is listed in the current
    /// task's `args` flags (which are not available here, so we defer to the
    /// global `validate` pass — this is a stub for inline checks).
    ///
    /// Full cross-task validation happens in [`validate`].
    fn check_placeholders(
        &self,
        _task_name: &str,
        _cmd: &str,
        _offset: usize,
    ) -> Result<(), ParseError> {
        // Placeholder validation requires the full TaskFlags, which are not
        // yet finalised at this point in parsing.  The `validate` function
        // performs the complete check after the AST is built.
        Ok(())
    }

    // ── List parsing ──────────────────────────────────────────────────────────

    /// Parse `[ IDENT (, IDENT)* ]` and return the identifiers.
    fn parse_ident_list(&mut self) -> Result<Vec<String>, ParseError> {
        // Expect `[`.
        if self.peek_kind() != &TokenKind::LBracket {
            let loc = offset_to_location(&self.src, self.peek().span.start);
            return Err(ParseError::UnclosedBracket { location: loc });
        }
        let bracket_start = self.peek().span.start;
        self.advance();

        let mut items: Vec<String> = Vec::new();

        loop {
            match self.peek_kind().clone() {
                TokenKind::RBracket => {
                    self.advance();
                    break;
                }
                TokenKind::Comma => {
                    self.advance();
                }
                TokenKind::Ident(name) => {
                    self.advance();
                    items.push(name);
                }
                TokenKind::Eof | TokenKind::Newline => {
                    let loc = offset_to_location(&self.src, bracket_start);
                    return Err(ParseError::UnclosedBracket { location: loc });
                }
                _ => {
                    let loc = offset_to_location(&self.src, self.peek().span.start);
                    return Err(ParseError::UnclosedBracket { location: loc });
                }
            }
        }

        Ok(items)
    }

    // ── Small helpers ─────────────────────────────────────────────────────────

    /// Consume an `=` token or return a descriptive error.
    fn expect_equals(&mut self, flag: &str) -> Result<(), ParseError> {
        if self.peek_kind() == &TokenKind::Equals {
            self.advance();
            Ok(())
        } else {
            let loc = offset_to_location(&self.src, self.peek().span.start);
            Err(ParseError::ExpectedEquals {
                flag: flag.to_string(),
                location: loc,
            })
        }
    }

    /// Consume a string literal token or return a descriptive error.
    fn expect_string(&mut self, flag: &str) -> Result<String, ParseError> {
        match self.peek_kind().clone() {
            TokenKind::StringLit(s) => {
                self.advance();
                Ok(s)
            }
            _ => {
                let loc = offset_to_location(&self.src, self.peek().span.start);
                Err(ParseError::ExpectedStringValue {
                    flag: flag.to_string(),
                    location: loc,
                })
            }
        }
    }

    /// Skip any number of consecutive Newline tokens.
    fn skip_newlines(&mut self) {
        while matches!(self.peek_kind(), TokenKind::Newline) {
            self.advance();
        }
    }

    fn advance(&mut self) {
        if self.cursor + 1 < self.tokens.len() {
            self.cursor += 1;
        }
    }

    fn peek(&self) -> &Token {
        &self.tokens[self.cursor]
    }

    fn peek_kind(&self) -> &TokenKind {
        &self.tokens[self.cursor].kind
    }

    fn is_at_end(&self) -> bool {
        matches!(self.peek_kind(), TokenKind::Eof)
    }
}

// ── Validation ────────────────────────────────────────────────────────────────

/// Second-pass validation over the complete AST.
///
/// Checks performed:
/// 1. Duplicate task names.
/// 2. Unknown dependencies (`deps=[…]` referencing non-existent tasks).
/// 3. Dependency cycles (topological sort).
/// 4. Undeclared `{{arg}}` placeholders in command strings.
fn validate(mossfile: &Mossfile, src: &str) -> Result<(), ParseError> {
    use std::collections::{HashMap, HashSet};

    // Build a name → task index map for O(1) lookups.
    let mut name_map: HashMap<&str, usize> = HashMap::new();
    for (i, task) in mossfile.tasks.iter().enumerate() {
        if let Some(_prev) = name_map.insert(task.name.as_str(), i) {
            let loc = offset_to_location(src, task.span.start);
            return Err(ParseError::DuplicateTask {
                name: task.name.clone(),
                location: loc,
            });
        }
    }

    // Validate deps and placeholders for every task.
    for task in &mossfile.tasks {
        // Unknown dependency check.
        for dep in &task.flags.deps {
            if !name_map.contains_key(dep.as_str()) {
                let loc = offset_to_location(src, task.span.start);
                return Err(ParseError::UnknownDependency {
                    task: task.name.clone(),
                    dep: dep.clone(),
                    location: loc,
                });
            }
        }

        // Placeholder check: every `{{name}}` must appear in `args`.
        let declared: HashSet<&str> = task.flags.args.iter().map(|s| s.as_str()).collect();
        for cmd in &task.commands {
            for placeholder in extract_placeholders(&cmd.raw) {
                if !declared.contains(placeholder) {
                    let loc = offset_to_location(src, cmd.span.start);
                    return Err(ParseError::UndeclaredArgument {
                        task: task.name.clone(),
                        arg: placeholder.to_string(),
                        location: loc,
                    });
                }
            }
        }
    }

    // Cycle detection via DFS with three-colour marking.
    // white = 0 (unvisited), grey = 1 (in stack), black = 2 (done).
    let n = mossfile.tasks.len();
    let mut colour = vec![0u8; n];

    fn dfs(
        node: usize,
        tasks: &[Task],
        name_map: &HashMap<&str, usize>,
        colour: &mut Vec<u8>,
    ) -> Result<(), ParseError> {
        colour[node] = 1; // grey — currently on the DFS stack

        for dep in &tasks[node].flags.deps {
            let dep_idx = *name_map.get(dep.as_str()).unwrap(); // already validated above
            if colour[dep_idx] == 1 {
                // Back-edge detected — build a readable cycle description.
                let cycle = format!("{} → {}", tasks[node].name, dep);
                return Err(ParseError::DependencyCycle { cycle });
            }
            if colour[dep_idx] == 0 {
                dfs(dep_idx, tasks, name_map, colour)?;
            }
        }

        colour[node] = 2; // black — fully explored
        Ok(())
    }

    for i in 0..n {
        if colour[i] == 0 {
            dfs(i, &mossfile.tasks, &name_map, &mut colour)?;
        }
    }

    Ok(())
}

/// Extract all `{{name}}` placeholder names from a command string.
fn extract_placeholders(cmd: &str) -> Vec<&str> {
    let mut results = Vec::new();
    let mut rest = cmd;
    while let Some(open) = rest.find("{{") {
        rest = &rest[open + 2..];
        if let Some(close) = rest.find("}}") {
            results.push(&rest[..close]);
            rest = &rest[close + 2..];
        } else {
            break;
        }
    }
    results
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse;

    #[test]
    fn test_simple_task() {
        let src = "task build:\n  cargo build --release\n";
        let mf = parse(src).unwrap();
        assert_eq!(mf.tasks.len(), 1);
        assert_eq!(mf.tasks[0].name, "build");
        assert_eq!(mf.tasks[0].commands[0].raw, "cargo build --release");
    }

    #[test]
    fn test_settings() {
        let src = "project = \"moss\"\nshell = \"bash\"\ntask build:\n  cargo build\n";
        let mf = parse(src).unwrap();
        assert_eq!(mf.settings.project, Some("moss".to_string()));
        assert_eq!(mf.settings.shell, Some("bash".to_string()));
    }

    #[test]
    fn test_parallel_flag() {
        let src = "task dev parallel:\n  cargo watch -x run\n  npm run dev\n";
        let mf = parse(src).unwrap();
        assert!(mf.tasks[0].flags.parallel);
        assert_eq!(mf.tasks[0].commands.len(), 2);
    }

    #[test]
    fn test_deps_flag() {
        let src = "task build:\n  cargo build\ntask test deps=[build]:\n  cargo test\n";
        let mf = parse(src).unwrap();
        assert_eq!(mf.tasks[1].flags.deps, vec!["build"]);
    }

    #[test]
    fn test_watch_flag() {
        let src = "task serve watch=\"src/**/*.rs\":\n  cargo run\n";
        let mf = parse(src).unwrap();
        assert_eq!(
            mf.tasks[0].flags.watch,
            Some(WatchConfig {
                glob: "src/**/*.rs".to_string()
            })
        );
    }

    #[test]
    fn test_ready_flag() {
        let src = "task serve ready=\"Listening on\":\n  cargo run\n";
        let mf = parse(src).unwrap();
        assert_eq!(mf.tasks[0].flags.ready, Some("Listening on".to_string()));
    }

    #[test]
    fn test_args_and_placeholders() {
        let src = "task deploy args=[env]:\n  ./deploy.sh {{env}}\n";
        let mf = parse(src).unwrap();
        assert_eq!(mf.tasks[0].flags.args, vec!["env"]);
    }

    #[test]
    fn test_description_line() {
        let src = "task test:\n  desc = \"Run all tests\"\n  cargo test\n";
        let mf = parse(src).unwrap();
        assert_eq!(mf.tasks[0].description, Some("Run all tests".to_string()));
    }

    #[test]
    fn test_duplicate_task_error() {
        let src = "task build:\n  cargo build\ntask build:\n  cargo build\n";
        assert!(matches!(parse(src), Err(ParseError::DuplicateTask { .. })));
    }

    #[test]
    fn test_unknown_dep_error() {
        let src = "task test deps=[build]:\n  cargo test\n";
        assert!(matches!(
            parse(src),
            Err(ParseError::UnknownDependency { .. })
        ));
    }

    #[test]
    fn test_cycle_detection() {
        let src = "task a deps=[b]:\n  echo a\ntask b deps=[a]:\n  echo b\n";
        assert!(matches!(
            parse(src),
            Err(ParseError::DependencyCycle { .. })
        ));
    }

    #[test]
    fn test_undeclared_arg_error() {
        let src = "task deploy:\n  ./deploy.sh {{env}}\n";
        assert!(matches!(
            parse(src),
            Err(ParseError::UndeclaredArgument { .. })
        ));
    }

    #[test]
    fn test_empty_body_error() {
        // A task with only a newline after the colon.
        let src = "task build:\ntask test:\n  cargo test\n";
        assert!(matches!(parse(src), Err(ParseError::EmptyTaskBody { .. })));
    }
}

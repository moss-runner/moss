//! Lexer for Mossfile source text.
//!
//! Converts raw source bytes into a flat [`Vec<Token>`] that the parser
//! consumes.  The lexer is intentionally simple: it does not build any tree
//! structure and does not track indentation depth — that is left to the parser.
//!
//! # Token overview
//!
//! ```text
//! task build deps=[lint] parallel:   <- Task, Ident, Deps, LBracket, Ident,
//!                                       RBracket, Parallel, Colon, Newline
//!   cargo build --release            <- Indent, Command("cargo build …"), Newline
//! ```
use crate::ast::Span;
use crate::error::{offset_to_location, ParseError};

// ── Token ─────────────────────────────────────────────────────────────────────

/// A single lexical unit with its position in the source.
#[derive(Debug, Clone, PartialEq)]
pub struct Token {
    pub kind: TokenKind,
    pub span: Span,
}

impl Token {
    fn new(kind: TokenKind, start: usize, end: usize) -> Self {
        Self {
            kind,
            span: Span::new(start, end),
        }
    }
}

/// All token kinds produced by the lexer.
#[derive(Debug, Clone, PartialEq)]
pub enum TokenKind {
    // ── Keywords / flags ──────────────────────────────────────────────────────
    /// The `task` keyword that starts a task declaration.
    Task,
    /// The `parallel` flag on a task header.
    Parallel,
    /// The `watch` flag identifier (value follows with `=`).
    Watch,
    /// The `ready` flag identifier (value follows with `=`).
    Ready,
    /// The `deps` flag identifier (value follows with `=[…]`).
    Deps,
    /// The `args` flag identifier (value follows with `=[…]`).
    Args,
    /// The `desc` key inside a task body (`desc = "…"`).
    Desc,

    // ── Settings keys ─────────────────────────────────────────────────────────
    /// `project` setting key.
    Project,
    /// `shell` setting key.
    Shell,

    // ── Punctuation ───────────────────────────────────────────────────────────
    /// `:`  — ends a task header.
    Colon,
    /// `=`  — separates a key from its value.
    Equals,
    /// `[`  — opens a list.
    LBracket,
    /// `]`  — closes a list.
    RBracket,
    /// `,`  — separates list items.
    Comma,

    // ── Values ────────────────────────────────────────────────────────────────
    /// A bare identifier: task name, dep name, arg name, etc.
    Ident(String),
    /// A quoted string literal (quotes stripped).
    StringLit(String),

    // ── Structure ─────────────────────────────────────────────────────────────
    /// Leading whitespace (2+ spaces or a tab) at the start of a body line.
    /// The contained `usize` is the number of leading spaces (tabs count as 1).
    Indent(usize),
    /// A logical newline (one or more blank lines collapsed into one token).
    Newline,

    // ── End of file ───────────────────────────────────────────────────────────
    Eof,
}

// ── Lexer ─────────────────────────────────────────────────────────────────────

/// Stateful lexer that walks the source string byte-by-byte.
pub struct Lexer<'src> {
    src: &'src str,
    /// Current byte position.
    pos: usize,
}

impl<'src> Lexer<'src> {
    pub fn new(src: &'src str) -> Self {
        Self { src, pos: 0 }
    }

    // ── Public entry point ────────────────────────────────────────────────────

    /// Lex the entire source and return all tokens, ending with [`TokenKind::Eof`].
    pub fn tokenize(mut self) -> Result<Vec<Token>, ParseError> {
        let mut tokens: Vec<Token> = Vec::new();

        while !self.is_at_end() {
            // Skip blank lines (lines that are entirely whitespace).
            if self.peek() == '\n' {
                self.advance();
                // Emit a Newline only if there is a non-newline token before it.
                if tokens.last().map(|t| &t.kind) != Some(&TokenKind::Newline) && !tokens.is_empty()
                {
                    let pos = self.pos;
                    tokens.push(Token::new(TokenKind::Newline, pos - 1, pos));
                }
                continue;
            }

            // Skip full-line comments (`# …`).
            if self.peek() == '#' {
                self.skip_line();
                continue;
            }

            // Detect indented body lines.
            if self.is_at_line_start() && (self.peek() == ' ' || self.peek() == '\t') {
                let tok = self.lex_indent_and_command()?;
                tokens.extend(tok);
                continue;
            }

            // Everything else is a top-level token.
            if let Some(tok) = self.lex_token()? {
                tokens.push(tok);
            }
        }

        // Always end with Newline then Eof so the parser has clean sentinels.
        if tokens.last().map(|t| &t.kind) != Some(&TokenKind::Newline) {
            tokens.push(Token::new(TokenKind::Newline, self.pos, self.pos));
        }
        tokens.push(Token::new(TokenKind::Eof, self.pos, self.pos));
        Ok(tokens)
    }

    // ── Indented lines ────────────────────────────────────────────────────────

    /// Lex one indented line, returning `[Indent(n), Command(raw), Newline]`.
    ///
    /// The "command" is the rest of the line after stripping the leading
    /// whitespace.  Inline comments (`# …`) are stripped.
    fn lex_indent_and_command(&mut self) -> Result<Vec<Token>, ParseError> {
        let line_start = self.pos;

        // Count leading spaces (tabs count as one unit).
        let mut depth = 0usize;
        while !self.is_at_end() && (self.peek() == ' ' || self.peek() == '\t') {
            depth += 1;
            self.advance();
        }

        // A line that is only whitespace — skip it.
        if self.is_at_end() || self.peek() == '\n' {
            if !self.is_at_end() {
                self.advance();
            }
            return Ok(vec![]);
        }

        // Skip inline comment lines.
        if self.peek() == '#' {
            self.skip_line();
            return Ok(vec![]);
        }

        // If the line starts with `desc`, tokenize it as structured tokens
        // so the parser can handle `desc = "…"` correctly.
        if self.src[self.pos..].starts_with("desc") {
            let after_desc = self.pos + 4;
            let next_ch = self.src[after_desc..].chars().next().unwrap_or('\0');
            if next_ch == ' ' || next_ch == '=' || next_ch == '\n' || next_ch == '\0' {
                let mut result = vec![Token::new(
                    TokenKind::Indent(depth),
                    line_start,
                    line_start + depth,
                )];
                // Tokenize the rest of the line normally (desc = "…").
                while !self.is_at_end() && self.peek() != '\n' {
                    if let Some(tok) = self.lex_token()? {
                        result.push(tok);
                    }
                }
                if !self.is_at_end() && self.peek() == '\n' {
                    self.advance();
                }
                result.push(Token::new(TokenKind::Newline, self.pos, self.pos + 1));
                return Ok(result);
            }
        }

        // Read the rest of the line as a raw command string.
        let cmd_start = self.pos;
        let cmd_raw = self.read_until_newline_or_comment();
        let cmd_end = self.pos;

        if !self.is_at_end() && self.peek() == '\n' {
            self.advance();
        }

        let cmd = cmd_raw.trim_end().to_string();
        if cmd.is_empty() {
            return Ok(vec![]);
        }

        Ok(vec![
            Token::new(TokenKind::Indent(depth), line_start, line_start + depth),
            Token::new(TokenKind::Ident(cmd), cmd_start, cmd_end),
            Token::new(TokenKind::Newline, cmd_end, cmd_end + 1),
        ])
    }

    // ── Top-level tokens ──────────────────────────────────────────────────────

    /// Lex one top-level token (keyword, punctuation, string, or identifier).
    fn lex_token(&mut self) -> Result<Option<Token>, ParseError> {
        // Skip horizontal whitespace between tokens on the same line.
        if self.peek() == ' ' || self.peek() == '\t' {
            while !self.is_at_end() && (self.peek() == ' ' || self.peek() == '\t') {
                self.advance();
            }
            return Ok(None);
        }

        let start = self.pos;

        match self.peek() {
            ':' => {
                self.advance();
                Ok(Some(Token::new(TokenKind::Colon, start, self.pos)))
            }
            '=' => {
                self.advance();
                Ok(Some(Token::new(TokenKind::Equals, start, self.pos)))
            }
            '[' => {
                self.advance();
                Ok(Some(Token::new(TokenKind::LBracket, start, self.pos)))
            }
            ']' => {
                self.advance();
                Ok(Some(Token::new(TokenKind::RBracket, start, self.pos)))
            }
            ',' => {
                self.advance();
                Ok(Some(Token::new(TokenKind::Comma, start, self.pos)))
            }

            '"' => {
                let s = self.lex_string()?;
                Ok(Some(Token::new(TokenKind::StringLit(s), start, self.pos)))
            }

            c if c.is_alphabetic() || c == '_' => {
                let word = self.lex_ident();
                let kind = Self::keyword_or_ident(word);
                Ok(Some(Token::new(kind, start, self.pos)))
            }

            c => {
                let loc = offset_to_location(self.src, start);
                Err(ParseError::UnexpectedChar {
                    ch: c,
                    location: loc,
                })
            }
        }
    }

    // ── String literals ───────────────────────────────────────────────────────

    /// Consume a `"…"` string and return its unquoted contents.
    fn lex_string(&mut self) -> Result<String, ParseError> {
        let start = self.pos;
        self.advance(); // consume opening `"`

        let mut buf = String::new();
        loop {
            if self.is_at_end() || self.peek() == '\n' {
                let loc = offset_to_location(self.src, start);
                return Err(ParseError::UnterminatedString { location: loc });
            }
            let c = self.current_char();
            self.advance();
            if c == '"' {
                break;
            }
            // Basic escape: `\"` inside a string.
            if c == '\\' && !self.is_at_end() && self.peek() == '"' {
                buf.push('"');
                self.advance();
                continue;
            }
            buf.push(c);
        }
        Ok(buf)
    }

    // ── Identifiers & keywords ────────────────────────────────────────────────

    /// Consume an identifier (letters, digits, `-`, `_`, `.`).
    fn lex_ident(&mut self) -> String {
        let start = self.pos;
        while !self.is_at_end() {
            let c = self.peek();
            if c.is_alphanumeric() || c == '_' || c == '-' || c == '.' {
                self.advance();
            } else {
                break;
            }
        }
        self.src[start..self.pos].to_string()
    }

    /// Map a raw identifier string to the appropriate [`TokenKind`].
    fn keyword_or_ident(word: String) -> TokenKind {
        match word.as_str() {
            "task" => TokenKind::Task,
            "parallel" => TokenKind::Parallel,
            "watch" => TokenKind::Watch,
            "ready" => TokenKind::Ready,
            "deps" => TokenKind::Deps,
            "args" => TokenKind::Args,
            "desc" => TokenKind::Desc,
            "project" => TokenKind::Project,
            "shell" => TokenKind::Shell,
            _ => TokenKind::Ident(word),
        }
    }

    // ── Helpers ───────────────────────────────────────────────────────────────

    /// Read characters until `\n` or `#` (inline comment), without consuming either.
    fn read_until_newline_or_comment(&mut self) -> String {
        let start = self.pos;
        while !self.is_at_end() && self.peek() != '\n' && self.peek() != '#' {
            self.advance();
        }
        // If we stopped at a `#` that is not at the start of the command, it
        // is an inline comment — skip to end of line without consuming `\n`.
        if !self.is_at_end() && self.peek() == '#' {
            while !self.is_at_end() && self.peek() != '\n' {
                self.advance();
            }
        }
        self.src[start..self.pos].to_string()
    }

    /// Advance past the rest of the current line (stops after consuming `\n`).
    fn skip_line(&mut self) {
        while !self.is_at_end() && self.peek() != '\n' {
            self.advance();
        }
        if !self.is_at_end() {
            self.advance();
        }
    }

    /// Return the character at the current position without advancing.
    fn peek(&self) -> char {
        self.src[self.pos..].chars().next().unwrap_or('\0')
    }

    /// Return the character at the current position (same as `peek`).
    fn current_char(&self) -> char {
        self.peek()
    }

    /// Advance past one UTF-8 character.
    fn advance(&mut self) {
        if let Some(c) = self.src[self.pos..].chars().next() {
            self.pos += c.len_utf8();
        }
    }

    /// True when the cursor is at the beginning of a line (pos == 0 or
    /// the previous byte was a newline).
    fn is_at_line_start(&self) -> bool {
        self.pos == 0 || self.src.as_bytes().get(self.pos - 1) == Some(&b'\n')
    }

    fn is_at_end(&self) -> bool {
        self.pos >= self.src.len()
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn lex(src: &str) -> Vec<TokenKind> {
        Lexer::new(src)
            .tokenize()
            .unwrap()
            .into_iter()
            .map(|t| t.kind)
            .collect()
    }

    #[test]
    fn test_simple_task_header() {
        let kinds = lex("task build:\n  cargo build\n");
        assert!(kinds.contains(&TokenKind::Task));
        assert!(kinds.contains(&TokenKind::Colon));
    }

    #[test]
    fn test_parallel_flag() {
        let kinds = lex("task dev parallel:\n  npm run dev\n");
        assert!(kinds.contains(&TokenKind::Parallel));
    }

    #[test]
    fn test_string_literal() {
        let kinds = lex("project = \"my-app\"\n");
        assert!(kinds.contains(&TokenKind::StringLit("my-app".to_string())));
    }

    #[test]
    fn test_unterminated_string_error() {
        let result = Lexer::new("project = \"oops\n").tokenize();
        assert!(matches!(result, Err(ParseError::UnterminatedString { .. })));
    }

    #[test]
    fn test_comment_skipped() {
        let kinds = lex("# this is a comment\ntask build:\n  cargo build\n");
        assert!(!kinds.contains(&TokenKind::Ident("#".to_string())));
        assert!(kinds.contains(&TokenKind::Task));
    }

    #[test]
    fn test_deps_list() {
        let kinds = lex("task test deps=[build, lint]:\n  cargo test\n");
        assert!(kinds.contains(&TokenKind::Deps));
        assert!(kinds.contains(&TokenKind::LBracket));
        assert!(kinds.contains(&TokenKind::RBracket));
    }
}

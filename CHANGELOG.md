# Changelog

All notable changes to Moss will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

---

## [Unreleased]

### Added
- Nothing yet — watch this space!

---

## [0.1.0] — 2026-05-15

### Added
- `moss-parser` — Mossfile lexer, AST, and recursive descent parser
  - Full syntax support: `task`, `deps`, `parallel`, `watch`, `ready`, `args`
  - Parse-time validation: duplicate tasks, unknown deps, dependency cycles, undeclared arguments
  - Informative error messages with line and column numbers
- `moss-core` — task execution engine
  - Sequential task runner with dependency resolution via topological sort (Kahn's algorithm)
  - Parallel executor with automatic fail-fast — if one command fails, all siblings are killed
  - Built-in file watcher via `notify` — no external tools required
  - Readiness signal detector — start dependent tasks only when a process is ready
  - `{{arg}}` placeholder substitution for task arguments
- `moss-cli` — command-line interface
  - `moss run <task> [args…]` — run a task and its dependencies
  - `moss list` — list all tasks with flags and descriptions
  - `moss completions <shell>` — generate shell completions for bash, zsh, fish, PowerShell
  - Pretty terminal output with colors and timing
  - `--verbose` flag to echo commands before running
  - `--file` flag to specify a custom Mossfile path
- `docs/syntax.md` — full Mossfile syntax reference
- `docs/migration.md` — migration guide from GNU Make and just

[Unreleased]: https://github.com/moss-runner/moss/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/moss-runner/moss/releases/tag/v0.1.0
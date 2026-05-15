<div align="center">

# 🌿 Moss

**A fast, modern task runner built in Rust.**

[![Crates.io](https://img.shields.io/crates/v/moss-cli.svg)](https://crates.io/crates/moss-cli)
[![License: MIT](https://img.shields.io/badge/license-MIT-green.svg)](LICENSE)
[![Build](https://github.com/aji-fullstack/moss/actions/workflows/ci.yml/badge.svg)](https://github.com/aji-fullstack/moss/actions)

[Getting Started](#getting-started) · [Syntax](#syntax-overview) · [Features](#features) · [Migration Guide](docs/migration.md) · [Docs](docs/syntax.md)

</div>

---

Moss is a task runner that fixes what's frustrating about `make` and `just`:

- **No `.PHONY`** — every task is always a command, never a file target
- **Parallel built-in** — run commands concurrently with one keyword, with automatic fail-fast
- **Watch mode built-in** — re-run tasks on file changes, no `watchexec` required
- **Readiness signals** — start dependent tasks only when a process is actually ready
- **Clear syntax** — readable at a glance, no tab sensitivity, no cryptic variables

---

## Getting Started

### Installation

**Via Cargo:**
```bash
cargo install moss-cli
```

**From source:**
```bash
git clone https://github.com/yourusername/moss
cd moss
cargo install --path crates/moss-cli
```

### Quick start

Create a `Mossfile` in your project root:

```text
project = "my-app"

task build:
  desc = "Build in release mode"
  cargo build --release

task test deps=[build]:
  desc = "Run all tests"
  cargo test

task dev parallel:
  desc = "Start all dev processes"
  cargo watch -x run
  npm run dev
```

Run a task:

```bash
moss run build
moss run test
moss run dev
moss list          # show all tasks
```

---

## Features

### Sequential tasks with dependencies

Tasks declare their dependencies explicitly. Moss resolves the correct
execution order automatically — no manual ordering required.

```text
task lint:
  cargo clippy -- -D warnings

task build deps=[lint]:
  cargo build --release

task test deps=[build]:
  cargo test --all-features
```

```bash
moss run test
# runs: lint → build → test
```

---

### Parallel execution with fail-fast

Add `parallel` to run every command in the task body concurrently.
If any command fails, all sibling processes are killed immediately.

```text
task dev parallel:
  cargo watch -x run
  npm run dev
  npx tailwindcss --watch
```

```bash
moss run dev
# all three processes start at the same time
# if one crashes, the others are stopped automatically
```

Compare this to Make's approach, which requires `&`, `wait`, and gives
no fail-fast guarantee:

```makefile
# Make — no fail-fast, verbose, error-prone
dev:
	cargo watch -x run &
	npm run dev &
	npx tailwindcss --watch &
	wait
```

---

### Built-in file watching

Re-run a task automatically when files change. No external tools needed.

```text
task build watch="src/**/*.rs":
  cargo build
```

```bash
moss run build
# builds once, then watches src/ and rebuilds on every .rs change
```

just requires installing `watchexec` separately and wiring it manually:

```just
# just — requires an external tool
watch:
  watchexec -w src -- just build
```

---

### Readiness signals

Start dependent tasks only when a long-running process signals it is ready,
based on a pattern in its output. No polling, no fixed sleep timers.

```text
task server ready="Listening on port":
  cargo run

task e2e deps=[server]:
  npx playwright test
```

```bash
moss run e2e
# starts `server`, waits until it prints "Listening on port",
# then starts `e2e` — no guessing, no sleep 5
```

---

### Task arguments

Declare named arguments and reference them in commands with `{{name}}`.

```text
task deploy args=[env]:
  desc = "Deploy to an environment"
  ./scripts/deploy.sh {{env}}
  echo "Deployed to {{env}} successfully"
```

```bash
moss run deploy production
moss run deploy staging
```

---

### Shell completion

Generate completions for your shell once and get `<TAB>` completion for
task names, flags, and subcommands.

```bash
# bash
moss completions bash >> ~/.bashrc

# zsh
moss completions zsh >> ~/.zshrc

# fish
moss completions fish > ~/.config/fish/completions/moss.fish
```

---

## Syntax overview

```text
# Mossfile

project = "my-app"   # optional project name
shell   = "bash"     # optional shell override (default: sh)

task <name> [flags…]:
  [desc = "Human-readable description"]
  <shell command>
  <shell command>
```

| Flag                    | Description                                              |
|-------------------------|----------------------------------------------------------|
| `deps=[t1, t2]`         | Tasks that must complete before this one                 |
| `parallel`              | Run all commands in the body concurrently                |
| `watch="glob"`          | Re-run when matching files change                        |
| `ready="pattern"`       | Signal readiness when stdout matches the pattern         |
| `args=[a, b]`           | Declare positional arguments (`{{a}}`, `{{b}}`)          |

See [docs/syntax.md](docs/syntax.md) for the full syntax reference.

---

## Comparison

| Feature                  | Moss  | just  | Make  |
|--------------------------|-------|-------|-------|
| Single binary            | ✅    | ✅    | ✅    |
| No runtime required      | ✅    | ✅    | ✅    |
| Cross-platform           | ✅    | ✅    | ⚠️    |
| Parallel execution       | ✅    | ⚠️    | ⚠️    |
| Fail-fast on parallel    | ✅    | ❌    | ❌    |
| Built-in watch mode      | ✅    | ❌    | ❌    |
| Readiness signals        | ✅    | ❌    | ❌    |
| Named task arguments     | ✅    | ✅    | ⚠️    |
| Dependency resolution    | ✅    | ✅    | ✅    |
| Cycle detection          | ✅    | ✅    | ❌    |
| Shell completion         | ✅    | ✅    | ❌    |
| No tab sensitivity       | ✅    | ✅    | ❌    |
| No `.PHONY` needed       | ✅    | ✅    | ❌    |

> ⚠️ = supported but requires workarounds or external tools

---

## Migrating from Make or just

Moss is designed to be easy to adopt. Most Makefiles and Justfiles can be
converted in minutes.

**Makefile → Mossfile:**
```makefile
# Before
.PHONY: build test
build:
	cargo build --release
test: build
	cargo test
```
```text
# After
task build:
  cargo build --release
task test deps=[build]:
  cargo test
```

**Justfile → Mossfile:**
```just
# Before
build:
  cargo build --release
test: build
  cargo test
```
```text
# After
task build:
  cargo build --release
task test deps=[build]:
  cargo test
```

See the full [Migration Guide](docs/migration.md) for every pattern.

---

## Project structure

Moss is a Cargo workspace with three crates:

```
moss/
├── crates/
│   ├── moss-parser/   — Mossfile lexer, AST, and parser
│   ├── moss-core/     — execution engine (runner, graph, parallel, watcher)
│   └── moss-cli/      — command-line interface
├── docs/
│   ├── syntax.md      — full syntax reference
│   └── migration.md   — migration guide from Make and just
└── tests/
    └── integration/   — end-to-end integration tests
```

---

## Contributing

Contributions are welcome! Please open an issue before submitting a large
pull request so we can discuss the approach.

```bash
# Clone and build
git clone https://github.com/yourusername/moss
cd moss
cargo build

# Run all tests
cargo test

# Run the integration tests
cargo test -p moss-integration-tests
```

---

## License

MIT — see [LICENSE](LICENSE) for details.

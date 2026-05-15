# Mossfile Syntax Reference

A `Mossfile` is the configuration file that Moss reads to understand your
project's tasks. It lives in the root of your project and is written in a
clean, minimal syntax designed to be readable at a glance.

---

## Table of Contents

- [File structure](#file-structure)
- [Settings](#settings)
- [Tasks](#tasks)
- [Flags](#flags)
  - [deps — dependencies](#deps--dependencies)
  - [parallel — concurrent commands](#parallel--concurrent-commands)
  - [watch — file watching](#watch--file-watching)
  - [ready — readiness signal](#ready--readiness-signal)
  - [args — task arguments](#args--task-arguments)
- [Task body](#task-body)
  - [Commands](#commands)
  - [Description](#description)
- [Comments](#comments)
- [Full example](#full-example)

---

## File structure

A `Mossfile` has two sections, in order:

1. **Settings** — optional key/value pairs that apply globally.
2. **Tasks** — one or more `task` declarations.

```text
# 1. Settings (optional)
project = "my-app"
shell   = "sh"

# 2. Tasks
task build:
  cargo build --release

task test deps=[build]:
  cargo test
```

---

## Settings

Settings are declared at the top of the file before any `task` block.
Each setting is a `key = "value"` pair on its own line.

| Key       | Default              | Description                                      |
|-----------|----------------------|--------------------------------------------------|
| `project` | _(none)_             | Human-readable project name, used in output.     |
| `shell`   | `sh` / `cmd` on Win  | Shell used to execute commands.                  |

```text
project = "my-app"
shell   = "bash"
```

> **Tip:** Setting `shell = "bash"` unlocks bash-specific syntax like
> `[[ ]]` conditionals and `${var:-default}` expansions in your commands.

---

## Tasks

A task is declared with the `task` keyword, followed by a name, optional
flags, and a colon. The body is indented with **two spaces**.

```text
task <name> [flags…]:
  [desc = "…"]
  <command>
  <command>
  …
```

### Rules

- Task names must be unique within a Mossfile.
- Task names may contain letters, digits, hyphens (`-`), underscores (`_`),
  and dots (`.`).
- The body must contain at least one command or a `desc` line.

```text
task build:
  cargo build --release

task run-dev:
  npm run dev

task v1.release:
  ./scripts/release.sh
```

---

## Flags

Flags appear on the task header line, between the task name and the colon.
Multiple flags can be combined freely.

```text
task serve watch="src/**/*.rs" ready="Listening on" deps=[build]:
  cargo run
```

---

### `deps` — dependencies

Declare tasks that must complete successfully before this task runs.
Dependencies are resolved recursively and executed in topological order.

```text
task lint:
  cargo clippy

task build deps=[lint]:
  cargo build --release

task test deps=[build]:
  cargo test
```

Running `moss run test` will execute: `lint` → `build` → `test`.

**Rules:**
- Dependency names must refer to tasks defined in the same Mossfile.
- Circular dependencies are detected at parse time and reported as an error.
- Multiple dependencies are comma-separated: `deps=[lint, format, check]`.

---

### `parallel` — concurrent commands

When the `parallel` flag is set, every command in the task body is spawned
**at the same time** instead of one after another.

```text
task dev parallel:
  cargo watch -x run
  npm run dev
  npx tailwindcss --watch
```

All three processes start simultaneously. If **any** of them exits with a
non-zero code, the remaining processes are killed immediately (fail-fast).

**Without `parallel`** (default), commands run sequentially and execution
stops at the first failure.

---

### `watch` — file watching

Re-run the task automatically whenever a file matching the glob pattern
changes. No external tools required — watching is built into Moss.

```text
task build watch="src/**/*.rs":
  cargo build
```

**Glob syntax:**

| Pattern        | Matches                                      |
|----------------|----------------------------------------------|
| `src/**/*.rs`  | All `.rs` files anywhere under `src/`        |
| `*.toml`       | All `.toml` files in the current directory   |
| `src/main.rs`  | Exactly one file                             |
| `templates/**` | All files anywhere under `templates/`        |

The task runs **once immediately**, then re-runs on every detected change.
Multiple file-system events within 100 ms are debounced into a single re-run.

---

### `ready` — readiness signal

Mark the task as "ready" the moment its output contains the given string.
Dependent tasks that are waiting will start as soon as the signal fires.

```text
task server ready="Listening on port":
  cargo run

task e2e deps=[server]:
  npx playwright test
```

`e2e` will not start until `server` prints `"Listening on port"` to stdout.

This is particularly useful for:
- API servers that need a moment to bind to a port.
- Database containers that print a ready message on startup.
- Any long-running process that has a predictable startup log line.

> **Note:** If the process exits without printing the ready string, Moss
> releases the signal anyway so dependent tasks are not blocked forever.

---

### `args` — task arguments

Declare named positional arguments that the task accepts at runtime.
Arguments are referenced in commands using `{{name}}` placeholders.

```text
task deploy args=[env]:
  ./scripts/deploy.sh {{env}}
  echo "Deployed to {{env}}"
```

```bash
moss run deploy production
moss run deploy staging
```

Multiple arguments are declared in order:

```text
task copy args=[src, dst]:
  cp -r {{src}} {{dst}}
```

```bash
moss run copy src/ backup/
```

**Rules:**
- Every `{{placeholder}}` in a command must be listed in `args=[…]`.
  Undeclared placeholders are caught at parse time.
- Arguments are positional — they are matched left-to-right to the names
  declared in `args=[…]`.

---

## Task body

### Commands

Each indented line in the task body is a shell command. Commands are passed
to the configured shell via `-c`, so any valid shell syntax works.

```text
task release:
  git tag v{{version}}
  git push origin v{{version}}
  cargo publish
```

Inline comments with `#` are stripped before the command is executed:

```text
task build:
  cargo build --release  # this comment is ignored
```

### Description

A task can have an optional description that appears in `moss list` output.
The `desc` line must be the **first** line of the body, before any commands.

```text
task test deps=[build]:
  desc = "Run the full test suite"
  cargo test --all-features
```

Output of `moss list`:

```
  test   dep   Run the full test suite
```

---

## Comments

Lines starting with `#` are comments and are ignored by the parser.
Comments can appear anywhere in the file.

```text
# This is a top-level comment.
project = "my-app"

# Build task — runs cargo in release mode.
task build:
  cargo build --release  # inline comments work too
```

---

## Full example

```text
# Mossfile — full example
project = "my-app"
shell   = "sh"

# ── Checks ────────────────────────────────────────────────────────────────────

task fmt:
  desc = "Check code formatting"
  cargo fmt --check

task lint:
  desc = "Run clippy lints"
  cargo clippy -- -D warnings

task check deps=[fmt, lint]:
  desc = "Run all static checks"
  cargo check

# ── Build ─────────────────────────────────────────────────────────────────────

task build deps=[check]:
  desc = "Build in release mode"
  cargo build --release

# ── Test ──────────────────────────────────────────────────────────────────────

task test deps=[build]:
  desc = "Run all tests"
  cargo test --all-features

# ── Development ───────────────────────────────────────────────────────────────

task dev parallel:
  desc = "Start all dev processes"
  cargo watch -x run
  npm run dev

task serve watch="src/**/*.rs" ready="Listening on":
  desc = "Run with auto-reload"
  cargo run

# ── Deploy ────────────────────────────────────────────────────────────────────

task deploy args=[env] deps=[test]:
  desc = "Deploy to an environment"
  ./scripts/deploy.sh {{env}}
```

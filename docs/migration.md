# Migration Guide

This guide helps you migrate existing task configurations from **GNU Make**
or **just** to Moss. Most migrations take less than five minutes.

---

## Table of Contents

- [From GNU Make](#from-gnu-make)
  - [Basic tasks](#basic-tasks-make)
  - [Dependencies](#dependencies-make)
  - [Variables](#variables-make)
  - [Phony targets](#phony-targets)
  - [Parallel execution](#parallel-execution-make)
  - [Common patterns](#common-patterns-make)
- [From just](#from-just)
  - [Basic recipes](#basic-recipes-just)
  - [Dependencies](#dependencies-just)
  - [Parameters](#parameters-just)
  - [Parallel execution](#parallel-execution-just)
  - [Watch mode](#watch-mode-just)
- [Concept mapping](#concept-mapping)
- [FAQ](#faq)

---

## From GNU Make

### Basic tasks (Make)

Make uses bare target names with a tab-indented body. Moss uses the explicit
`task` keyword and two-space indentation.

**Before (Makefile):**
```makefile
build:
	cargo build --release

test:
	cargo test

clean:
	rm -rf target/
```

**After (Mossfile):**
```text
task build:
  cargo build --release

task test:
  cargo test

task clean:
  rm -rf target/
```

> **Key difference:** Moss uses **spaces**, not tabs. Tabs in a Mossfile
> are accepted but spaces are the convention.

---

### Dependencies (Make)

**Before (Makefile):**
```makefile
build: lint
	cargo build --release

test: build
	cargo test

lint:
	cargo clippy
```

**After (Mossfile):**
```text
task lint:
  cargo clippy

task build deps=[lint]:
  cargo build --release

task test deps=[build]:
  cargo test
```

> **Key difference:** Dependencies are declared **inline on the header line**
> with `deps=[…]` instead of after the target name. This makes the
> dependency relationship explicit and easy to scan.

---

### Variables (Make)

Make variables become shell variables or Moss `args`.

**Before (Makefile):**
```makefile
ENV ?= development

deploy:
	./deploy.sh $(ENV)
```

**After (Mossfile):**
```text
task deploy args=[env]:
  ./deploy.sh {{env}}
```

```bash
# Make
make deploy ENV=production

# Moss
moss run deploy production
```

For variables that never change (e.g. compiler flags), just inline them
directly in the command or use a shell variable in your command string.

---

### Phony targets

Make requires `.PHONY` declarations to prevent conflicts with files of the
same name. Moss has no concept of file targets — **every task is always
a command**, never a file dependency.

**Before (Makefile):**
```makefile
.PHONY: build test clean deploy

build:
	cargo build

test:
	cargo test
```

**After (Mossfile):**
```text
task build:
  cargo build

task test:
  cargo test
```

Simply remove all `.PHONY` declarations — they are not needed in Moss.

---

### Parallel execution (Make)

Make parallelism requires the `-j` flag at the call site and careful use
of `&` and `wait` inside recipes. Moss parallelism is declared on the task.

**Before (Makefile):**
```makefile
dev:
	cargo watch -x run &
	npm run dev &
	wait
```

**After (Mossfile):**
```text
task dev parallel:
  cargo watch -x run
  npm run dev
```

Moss also adds **fail-fast**: if either process exits with an error, the
other is killed automatically. Make's `&`/`wait` approach does not do this.

---

### Common patterns (Make)

#### Default target

**Before (Makefile):**
```makefile
.DEFAULT_GOAL := build

build:
	cargo build
```

**After (Mossfile):**
```text
task build:
  cargo build
```

```bash
# Moss: just specify the task explicitly
moss run build
```

#### Multi-line commands

**Before (Makefile):**
```makefile
release:
	git add .
	git commit -m "release"
	git push
	cargo publish
```

**After (Mossfile):**
```text
task release:
  git add .
  git commit -m "release"
  git push
  cargo publish
```

Each line in a Moss task body is a separate shell invocation, identical to
how Make handles multi-line recipes.

---

## From just

### Basic recipes (just)

**Before (Justfile):**
```just
build:
  cargo build --release

test:
  cargo test
```

**After (Mossfile):**
```text
task build:
  cargo build --release

task test:
  cargo test
```

> **Key difference:** Add the `task` keyword before each name. Everything
> else stays the same.

---

### Dependencies (just)

**Before (Justfile):**
```just
lint:
  cargo clippy

build: lint
  cargo build --release

test: build
  cargo test
```

**After (Mossfile):**
```text
task lint:
  cargo clippy

task build deps=[lint]:
  cargo build --release

task test deps=[build]:
  cargo test
```

> **Key difference:** Dependencies move from after the task name to an
> explicit `deps=[…]` flag. This makes them easier to spot in longer files.

---

### Parameters (just)

**Before (Justfile):**
```just
deploy env="development":
  ./deploy.sh {{env}}

greet name surname:
  echo "Hello, {{name}} {{surname}}!"
```

**After (Mossfile):**
```text
task deploy args=[env]:
  ./deploy.sh {{env}}

task greet args=[name, surname]:
  echo "Hello, {{name}} {{surname}}!"
```

```bash
# just
just deploy production
just greet John Doe

# Moss
moss run deploy production
moss run greet John Doe
```

> **Key difference:** just allows default values for parameters; Moss
> currently requires all declared arguments to be provided at runtime.

---

### Parallel execution (just)

just requires splitting parallel work across multiple recipes with the
`[parallel]` attribute. Moss keeps it in a single task.

**Before (Justfile):**
```just
[parallel]
dev: frontend backend

frontend:
  npm run dev

backend:
  cargo watch -x run
```

**After (Mossfile):**
```text
task dev parallel:
  npm run dev
  cargo watch -x run
```

Moss is more concise — you do not need to define separate tasks just to run
them in parallel. The commands live together in one block.

---

### Watch mode (just)

just has no built-in watch mode. Users must install and invoke `watchexec`
or a similar external tool manually.

**Before (Justfile):**
```just
# requires `watchexec` to be installed separately
watch:
  watchexec -w src -- just build
```

**After (Mossfile):**
```text
task build watch="src/**/*.rs":
  cargo build
```

Watch mode is built into Moss — no extra tools required. The glob pattern
is declared on the task header and Moss handles the rest.

---

## Concept mapping

| Concept                  | GNU Make                  | just                  | Moss                          |
|--------------------------|---------------------------|-----------------------|-------------------------------|
| Declare a task           | `target:`                 | `recipe:`             | `task name:`                  |
| Task body indentation    | Tab (required)            | 2 spaces              | 2 spaces                      |
| Dependencies             | `target: dep1 dep2`       | `recipe: dep1 dep2`   | `task name deps=[dep1, dep2]` |
| Parallel execution       | `cmd1 & cmd2 & wait`      | `[parallel]` + split  | `task name parallel:`         |
| Positional arguments     | `$(ARG)` via env/override | `recipe arg:`         | `task name args=[arg]:`       |
| Argument placeholder     | `$(ARG)`                  | `{{arg}}`             | `{{arg}}`                     |
| Watch mode               | Not built-in              | Not built-in          | `task name watch="glob":`     |
| Readiness signal         | Not supported             | Not supported         | `task name ready="pattern":`  |
| Skip file conflicts      | `.PHONY`                  | Not needed            | Not needed                    |
| Comments                 | `#`                       | `#`                   | `#`                           |
| Default shell            | `sh`                      | `sh`                  | `sh` (configurable)           |

---

## FAQ

**Q: Can I keep my Makefile and Mossfile side by side?**

Yes. Moss only reads `Mossfile` (or the path given with `--file`). Your
existing `Makefile` is untouched and both tools can coexist in the same
project during a gradual migration.

---

**Q: Does Moss support all shell syntax that Make and just support?**

Yes. Commands are passed to the shell with `-c`, so any syntax your shell
supports works in Moss — pipes, redirections, subshells, conditionals, etc.

```text
task check:
  cargo fmt --check && cargo clippy -- -D warnings
```

---

**Q: just supports default argument values — does Moss?**

Not yet. All declared arguments must be provided at runtime. Default values
are planned for a future release.

---

**Q: How do I run Moss tasks from a CI pipeline (GitHub Actions, etc.)?**

Install the `moss` binary in your workflow and call it like any other tool:

```yaml
- name: Install Moss
  run: cargo install moss-cli

- name: Run tests
  run: moss run test
```

---

**Q: My Makefile uses `$(shell ...)` to run commands at parse time. What do I use in Moss?**

Use shell command substitution directly inside your commands:

```text
task info:
  echo "Git branch: $(git rev-parse --abbrev-ref HEAD)"
```

This runs in the shell at execution time, which is equivalent for most
real-world use cases.

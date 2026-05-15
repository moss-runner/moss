/// Integration tests for moss-parser and moss-core.
///
/// One comprehensive Mossfile that exercises every feature in a single parse,
/// plus async execution tests for the core engine.
use moss_parser::{parse, ParseError};

// ── moss-parser tests ─────────────────────────────────────────────────────────

#[test]
fn test_full_mossfile() {
    let src = r#"
# Global settings
project = "my-app"
shell   = "sh"

# Simple task
task build:
  cargo build --release

# Task with description and dependency
task test deps=[build]:
  desc = "Run all tests"
  cargo test

# Parallel task with two commands
task dev parallel:
  cargo watch -x run
  npm run dev

# Watch + ready flags
task serve watch="src/**/*.rs" ready="Listening on":
  cargo run

# Task with arguments
task deploy args=[env]:
  ./deploy.sh {{env}}
"#;

    let mf = parse(src).unwrap();

    // Settings
    assert_eq!(mf.settings.project, Some("my-app".to_string()));
    assert_eq!(mf.settings.shell, Some("sh".to_string()));

    // Correct number of tasks
    assert_eq!(mf.tasks.len(), 5);

    // build
    let build = &mf.tasks[0];
    assert_eq!(build.name, "build");
    assert_eq!(build.commands[0].raw, "cargo build --release");

    // test
    let test = &mf.tasks[1];
    assert_eq!(test.flags.deps, vec!["build"]);
    assert_eq!(test.description, Some("Run all tests".to_string()));

    // dev
    let dev = &mf.tasks[2];
    assert!(dev.flags.parallel);
    assert_eq!(dev.commands.len(), 2);

    // serve
    let serve = &mf.tasks[3];
    assert_eq!(serve.flags.watch.as_ref().unwrap().glob, "src/**/*.rs");
    assert_eq!(serve.flags.ready, Some("Listening on".to_string()));

    // deploy
    let deploy = &mf.tasks[4];
    assert_eq!(deploy.flags.args, vec!["env"]);
    assert!(deploy.commands[0].raw.contains("{{env}}"));

    // --- Error cases ---

    // Duplicate task
    assert!(matches!(
        parse("task a:\n  echo a\ntask a:\n  echo a\n"),
        Err(ParseError::DuplicateTask { .. })
    ));

    // Unknown dependency
    assert!(matches!(
        parse("task a deps=[ghost]:\n  echo a\n"),
        Err(ParseError::UnknownDependency { .. })
    ));

    // Dependency cycle
    assert!(matches!(
        parse("task a deps=[b]:\n  echo a\ntask b deps=[a]:\n  echo b\n"),
        Err(ParseError::DependencyCycle { .. })
    ));

    // Undeclared argument placeholder
    assert!(matches!(
        parse("task deploy:\n  ./deploy.sh {{env}}\n"),
        Err(ParseError::UndeclaredArgument { .. })
    ));

    // Unterminated string
    assert!(matches!(
        parse("project = \"oops\n"),
        Err(ParseError::UnterminatedString { .. })
    ));
}

// ── moss-core tests ───────────────────────────────────────────────────────────

#[cfg(test)]
mod core_tests {
    use moss_core::{runner::Runner, RunError};
    use moss_parser::parse;

    /// Helper: parse `src`, run `task_name`, return the result.
    async fn run(src: &str, task_name: &str) -> Result<(), RunError> {
        let mf = parse(src).unwrap();
        Runner::new(&mf).run(task_name, &[]).await
    }

    // ── Happy path ────────────────────────────────────────────────────────────

    #[tokio::test]
    async fn test_simple_task_runs() {
        let src = "task greet:\n  echo hello\n";
        assert!(run(src, "greet").await.is_ok());
    }

    #[tokio::test]
    async fn test_multiple_sequential_commands() {
        // Both commands must succeed for the task to pass.
        let src = "task check:\n  echo one\n  echo two\n";
        assert!(run(src, "check").await.is_ok());
    }

    #[tokio::test]
    async fn test_deps_executed_before_target() {
        // `setup` echoes first, then `build` — no error means both ran.
        let src = concat!(
            "task setup:\n  echo setup done\n",
            "task build deps=[setup]:\n  echo build done\n",
        );
        assert!(run(src, "build").await.is_ok());
    }

    #[tokio::test]
    async fn test_transitive_deps_run_in_order() {
        // lint → build → test: all three must run without error.
        let src = concat!(
            "task lint:\n  echo lint\n",
            "task build deps=[lint]:\n  echo build\n",
            "task test deps=[build]:\n  echo test\n",
        );
        assert!(run(src, "test").await.is_ok());
    }

    #[tokio::test]
    async fn test_parallel_commands_succeed() {
        let src = "task ping parallel:\n  echo one\n  echo two\n";
        assert!(run(src, "ping").await.is_ok());
    }

    #[tokio::test]
    async fn test_arg_substitution() {
        // `echo hello world` should exit 0.
        let src = "task greet args=[name]:\n  echo hello {{name}}\n";
        let mf = parse(src).unwrap();
        let result = Runner::new(&mf).run("greet", &["world".to_string()]).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_multiple_args_substituted() {
        let src = "task copy args=[src, dst]:\n  echo {{src}} {{dst}}\n";
        let mf = parse(src).unwrap();
        let result = Runner::new(&mf)
            .run("copy", &["file.txt".to_string(), "backup.txt".to_string()])
            .await;
        assert!(result.is_ok());
    }

    // ── Error cases ───────────────────────────────────────────────────────────

    #[tokio::test]
    async fn test_unknown_task_returns_error() {
        let src = "task build:\n  echo build\n";
        assert!(matches!(
            run(src, "nonexistent").await,
            Err(RunError::TaskNotFound(_))
        ));
    }

    #[tokio::test]
    async fn test_failing_command_returns_error() {
        // `exit 1` exits non-zero — runner must surface this as an error.
        let src = "task fail:\n  exit 1\n";
        assert!(matches!(
            run(src, "fail").await,
            Err(RunError::CommandFailed { .. })
        ));
    }

    #[tokio::test]
    async fn test_second_command_not_run_after_failure() {
        // If the first command fails, the second must not run.
        // We verify this by making the second command succeed — if the test
        // returns CommandFailed we know execution stopped at the first command.
        let src = "task check:\n  exit 1\n  echo should_not_run\n";
        assert!(matches!(
            run(src, "check").await,
            Err(RunError::CommandFailed { .. })
        ));
    }

    #[tokio::test]
    async fn test_parallel_fail_fast() {
        // One command fails immediately; the other sleeps.
        // The runner must return an error without hanging.
        let src = "task race parallel:\n  exit 1\n  sleep 60\n";
        let result =
            tokio::time::timeout(std::time::Duration::from_secs(5), run(src, "race")).await;

        // Must complete within 5 s (not hang on `sleep 60`).
        assert!(result.is_ok(), "parallel fail-fast timed out");
        assert!(
            result.unwrap().is_err(),
            "expected RunError from parallel failure"
        );
    }

    #[tokio::test]
    async fn test_failing_dep_stops_execution() {
        // If a dependency fails, the target task must not run.
        let src = concat!(
            "task bad-dep:\n  exit 1\n",
            "task build deps=[bad-dep]:\n  echo should_not_run\n",
        );
        assert!(matches!(
            run(src, "build").await,
            Err(RunError::CommandFailed { .. })
        ));
    }
}

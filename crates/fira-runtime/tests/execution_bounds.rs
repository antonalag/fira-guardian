//! Task 14 — execution safety bounds: bounded/interruptible execution, stdin
//! isolation, partial-output capture, and the non-terminating-mechanism
//! NOT_RUN representation. FS/process only; no network. Uses tiny POSIX fixture
//! commands (`sleep`, `cat`, `sh`) and a sub-second injected bound.

use std::time::{Duration, Instant};

use fira_core::execution::VerificationMechanism;
use fira_core::interfaces::CommandExecutor;
use fira_core::model::{AlwaysTrue, ExecutionOutcome, IdRef, VerificationMechanismSource};
use fira_runtime::{discover, DiscoveredMechanism, MechanismRegistry, ProjectCommandExecutor};
use tempfile::tempdir;

/// Build a one-off mechanism with an explicit argv (bypasses discovery so these
/// tests are deterministic and host-manifest-independent). `reason` is the
/// Task-14 runtime-local non-terminating marker.
fn mech(id: &str, argv: &[&str], reason: Option<&str>) -> DiscoveredMechanism {
    DiscoveredMechanism {
        mechanism: VerificationMechanism {
            id: IdRef(id.to_string()),
            source: VerificationMechanismSource::Script,
            source_locator: "test:1".to_string(),
            command: argv.join(" "),
            declared_by_project: AlwaysTrue,
        },
        argv: argv.iter().map(|s| s.to_string()).collect(),
        non_terminating_reason: reason.map(str::to_string),
    }
}

/// Returns the executor **and** the temp dir guard; the caller must keep the
/// guard alive for the duration of the run (dropping it deletes the cwd and the
/// spawn would fail with ENOENT).
fn executor(
    entries: Vec<DiscoveredMechanism>,
    bound: Duration,
) -> (ProjectCommandExecutor, tempfile::TempDir) {
    let dir = tempdir().unwrap();
    let exec = ProjectCommandExecutor::with_bound(
        MechanismRegistry::from_entries(entries),
        dir.path(),
        bound,
    );
    (exec, dir)
}

/// AC-1: a command that does not terminate within the bound is actually
/// interrupted and reported as TIMEOUT — in ~the bound, not indefinitely.
#[test]
fn hung_command_is_bounded_and_timed_out() {
    let (exec, _dir) = executor(
        vec![mech("hang", &["sleep", "60"], None)],
        Duration::from_millis(300),
    );
    let start = Instant::now();
    let result = exec
        .run_existing(&IdRef("hang".to_string()))
        .expect("returns");
    let elapsed = start.elapsed();

    assert_eq!(
        result.outcome,
        ExecutionOutcome::Timeout,
        "a hang ⇒ TIMEOUT"
    );
    assert!(
        result.blocking_condition.is_some(),
        "TIMEOUT carries a blocking_condition"
    );
    // Returned promptly after the bound — proof it interrupted the live child,
    // not an elapsed check after a 60s `output()` (which would take ~60s).
    assert!(
        elapsed < Duration::from_secs(10),
        "must return shortly after the 300ms bound, took {elapsed:?}"
    );
    // Fail-safe: a timeout is never PASS.
    assert_ne!(result.outcome, ExecutionOutcome::Passed);
}

/// AC-4: partial stdout produced before the hang is captured on timeout.
#[test]
fn partial_output_is_captured_on_timeout() {
    let (exec, _dir) = executor(
        vec![mech(
            "chatty-hang",
            &["sh", "-c", "echo FIRA_PARTIAL_MARKER; sleep 60"],
            None,
        )],
        Duration::from_millis(400),
    );
    let result = exec
        .run_existing(&IdRef("chatty-hang".to_string()))
        .expect("returns");
    assert_eq!(result.outcome, ExecutionOutcome::Timeout);
    assert!(
        result.captured_output.contains("FIRA_PARTIAL_MARKER"),
        "partial output must be captured; got {:?}",
        result.captured_output
    );
}

/// AC-3: stdin is isolated — a command that reads stdin to EOF (`cat`) does not
/// hang on the audit process's stdin; it sees EOF and exits normally.
#[test]
fn stdin_is_isolated_reader_does_not_hang() {
    let (exec, _dir) = executor(
        vec![mech("reads-stdin", &["cat"], None)],
        // A generous bound: if stdin were inherited and blocking, this would
        // TIMEOUT; with stdin null, `cat` exits immediately (PASSED) well under it.
        Duration::from_secs(5),
    );
    let start = Instant::now();
    let result = exec
        .run_existing(&IdRef("reads-stdin".to_string()))
        .expect("returns");
    assert!(
        start.elapsed() < Duration::from_secs(3),
        "cat with closed stdin must exit immediately, not block on the bound"
    );
    assert_eq!(
        result.outcome,
        ExecutionOutcome::Passed,
        "cat reading closed stdin exits 0"
    );
}

/// A normally-terminating command still runs and reports PASSED/FAILED.
#[test]
fn normal_command_runs_to_completion() {
    let (exec, _dir) = executor(vec![mech("ok", &["true"], None)], Duration::from_secs(5));
    let result = exec
        .run_existing(&IdRef("ok".to_string()))
        .expect("returns");
    assert_eq!(result.outcome, ExecutionOutcome::Passed);
    assert!(result.blocking_condition.is_none());
}

/// AC-7/AC-8: a mechanism marked non-terminating is NOT executed — it returns
/// NOT_RUN with a reason, remains in the registry as evidence, and no command is
/// synthesized. (Here the argv is `sleep 60`; if it had been executed the test
/// would hang/timeout — proving it was skipped, not run.)
#[test]
fn non_terminating_mechanism_is_not_run() {
    let (exec, _dir) = executor(
        vec![mech(
            "watcher",
            &["sleep", "60"],
            Some("watcher (declared --watch/-w/watch)"),
        )],
        // Tiny bound: if it were (wrongly) executed, we'd see TIMEOUT, not NOT_RUN.
        Duration::from_millis(200),
    );
    let start = Instant::now();
    let result = exec
        .run_existing(&IdRef("watcher".to_string()))
        .expect("returns");
    assert_eq!(
        result.outcome,
        ExecutionOutcome::NotRun,
        "skipped, not executed"
    );
    assert!(
        result
            .blocking_condition
            .as_deref()
            .unwrap_or("")
            .contains("not executed"),
        "NOT_RUN states why; got {:?}",
        result.blocking_condition
    );
    assert!(
        start.elapsed() < Duration::from_secs(1),
        "NOT_RUN returns immediately (no process spawned)"
    );
    // Still represented in the registry as project evidence.
    let ids: Vec<String> = exec
        .discover_mechanisms()
        .unwrap()
        .into_iter()
        .map(|m| m.id.0)
        .collect();
    assert!(
        ids.contains(&"watcher".to_string()),
        "stays in the registry"
    );
}

/// AC-9 (regression): a durable-agents-shaped package.json — one-shot scripts
/// plus `test:watch: vitest --watch` — discovers `test:watch` as non-terminating
/// (NOT executed) while the one-shot scripts remain executable. End-to-end via
/// real discovery + a tiny bound; the run terminates without hanging.
#[test]
fn durable_agents_shaped_project_terminates() {
    let dir = tempdir().unwrap();
    std::fs::write(
        dir.path().join("package.json"),
        r#"{
          "name": "demo",
          "scripts": {
            "build": "tsup",
            "test": "vitest run",
            "test:coverage": "vitest run --coverage",
            "test:watch": "vitest --watch",
            "typecheck": "tsc --noEmit",
            "dev": "vite dev"
          }
        }"#,
    )
    .unwrap();

    let found = discover(dir.path());
    let by_id = |id: &str| {
        found
            .iter()
            .find(|m| m.mechanism.id.0 == id)
            .unwrap_or_else(|| panic!("missing {id}"))
    };

    // The watcher and dev server are flagged non-terminating (from their bodies).
    assert!(by_id("npm-test:watch").non_terminating_reason.is_some());
    assert!(by_id("npm-dev").non_terminating_reason.is_some());
    // One-shot scripts are executable.
    assert!(by_id("npm-test").non_terminating_reason.is_none());
    assert!(by_id("npm-test:coverage").non_terminating_reason.is_none());
    assert!(by_id("npm-build").non_terminating_reason.is_none());
    assert!(by_id("npm-typecheck").non_terminating_reason.is_none());

    // Running the flagged watcher returns NOT_RUN immediately (no hang), using a
    // tiny bound so that an accidental execution would surface as TIMEOUT.
    let exec = ProjectCommandExecutor::with_bound(
        MechanismRegistry::from_entries(found.clone()),
        dir.path(),
        Duration::from_millis(200),
    );
    let watch = exec
        .run_existing(&IdRef("npm-test:watch".to_string()))
        .expect("returns");
    assert_eq!(watch.outcome, ExecutionOutcome::NotRun);
}

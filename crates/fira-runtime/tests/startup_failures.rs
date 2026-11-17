//! Failure classification in RUNTIME: how an attempt to run a declared
//! mechanism is normalized into `(outcome, classification)`.
//!
//! Tests that spawn processes depend on `PATH`, which is process-global, so every
//! test takes a static lock and restores `PATH` on drop. `PATH` is only ever set
//! to an existing, empty temporary directory, never to the empty string (an empty
//! component of `PATH` means the current directory, i.e. the audited project).

use std::ffi::OsString;
use std::sync::{Mutex, MutexGuard};
use std::time::Duration;

use fira_core::execution::{ExecutionResult, VerificationMechanism};
use fira_core::interfaces::{CapabilityError, CommandExecutor};
use fira_core::model::{
    AlwaysTrue, ExecutionClassification, ExecutionOutcome, IdRef, VerificationMechanismSource,
};
use fira_runtime::{DiscoveredMechanism, MechanismRegistry, ProjectCommandExecutor};
use tempfile::{tempdir, TempDir};

static ENV_LOCK: Mutex<()> = Mutex::new(());

/// Holds the environment lock and restores `PATH` when dropped (the restore runs
/// before the lock field is released).
struct EnvGuard {
    saved: Option<OsString>,
    _lock: MutexGuard<'static, ()>,
}

impl EnvGuard {
    /// Lock the environment and leave `PATH` as it is.
    fn keep_path() -> Self {
        let lock = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        EnvGuard {
            saved: std::env::var_os("PATH"),
            _lock: lock,
        }
    }

    /// Lock the environment and point `PATH` at `dir` (an existing directory).
    fn set_path(dir: &std::path::Path) -> Self {
        let guard = Self::keep_path();
        std::env::set_var("PATH", dir);
        guard
    }
}

impl Drop for EnvGuard {
    fn drop(&mut self) {
        match &self.saved {
            Some(v) => std::env::set_var("PATH", v),
            None => std::env::remove_var("PATH"),
        }
    }
}

fn mech(id: &str, argv: &[&str]) -> DiscoveredMechanism {
    DiscoveredMechanism {
        mechanism: VerificationMechanism {
            id: IdRef(id.to_string()),
            source: VerificationMechanismSource::Script,
            source_locator: "test:1".to_string(),
            command: argv.join(" "),
            declared_by_project: AlwaysTrue,
        },
        argv: argv.iter().map(|s| s.to_string()).collect(),
        non_terminating_reason: None,
    }
}

fn executor_in(
    root: &std::path::Path,
    entries: Vec<DiscoveredMechanism>,
) -> ProjectCommandExecutor {
    ProjectCommandExecutor::new(MechanismRegistry::from_entries(entries), root)
}

fn run(exec: &ProjectCommandExecutor, id: &str) -> ExecutionResult {
    exec.run_existing(&IdRef(id.to_string()))
        .expect("a start failure is a result, not an Err")
}

fn empty_dir() -> TempDir {
    tempdir().unwrap()
}

/// AC-5: a FIRA-selected launcher missing from `PATH`, with the project
/// directory accessible, is ERROR + INFRA_LEVEL.
#[test]
fn missing_fira_launcher_is_infra() {
    let path_dir = empty_dir();
    let project = tempdir().unwrap();
    let _env = EnvGuard::set_path(path_dir.path());
    let exec = executor_in(project.path(), vec![mech("make-noop", &["make", "noop"])]);

    let r = run(&exec, "make-noop");
    assert_eq!(r.outcome, ExecutionOutcome::Error);
    assert_eq!(r.classification, Some(ExecutionClassification::InfraLevel));
    assert!(r.blocking_condition.is_some());
}

/// AC-5: a FIRA-selected launcher that exists but has no execute permission is
/// ERROR + INFRA_LEVEL (the OS reports permission denied).
#[cfg(unix)]
#[test]
fn non_executable_fira_launcher_is_infra() {
    use std::os::unix::fs::PermissionsExt;

    let path_dir = empty_dir();
    let launcher = path_dir.path().join("make");
    std::fs::write(&launcher, "#!/bin/sh\nexit 0\n").unwrap();
    std::fs::set_permissions(&launcher, std::fs::Permissions::from_mode(0o644)).unwrap();
    let project = tempdir().unwrap();
    let _env = EnvGuard::set_path(path_dir.path());
    let exec = executor_in(project.path(), vec![mech("make-noop", &["make", "noop"])]);

    let r = run(&exec, "make-noop");
    assert_eq!(r.outcome, ExecutionOutcome::Error);
    assert_eq!(r.classification, Some(ExecutionClassification::InfraLevel));
}

/// AC-6: a missing program that FIRA did not select is UNCLASSIFIED.
#[test]
fn unknown_launcher_missing_program_is_unclassified() {
    let path_dir = empty_dir();
    let project = tempdir().unwrap();
    let _env = EnvGuard::set_path(path_dir.path());
    let exec = executor_in(
        project.path(),
        vec![mech("custom", &["definitely-not-a-fira-launcher"])],
    );

    let r = run(&exec, "custom");
    assert_eq!(r.outcome, ExecutionOutcome::Error);
    assert_eq!(
        r.classification,
        Some(ExecutionClassification::Unclassified)
    );
}

/// AC-7: a missing project directory is UNCLASSIFIED even for an allowlisted
/// launcher, and nothing is started.
#[test]
fn missing_project_path_is_unclassified() {
    let path_dir = empty_dir();
    let parent = tempdir().unwrap();
    let missing = parent.path().join("gone");
    let _env = EnvGuard::set_path(path_dir.path());
    let exec = executor_in(&missing, vec![mech("make-noop", &["make", "noop"])]);

    let r = run(&exec, "make-noop");
    assert_eq!(r.outcome, ExecutionOutcome::Error);
    assert_eq!(
        r.classification,
        Some(ExecutionClassification::Unclassified)
    );
    assert!(r
        .blocking_condition
        .as_deref()
        .unwrap()
        .contains("not started"));
}

/// AC-29: a project directory with read but no search permission cannot be
/// verified, so the result is UNCLASSIFIED. When the process is privileged the
/// permission bits are not enforced and the test reports "not exercised".
#[cfg(unix)]
#[test]
fn unsearchable_project_dir_is_unclassified() {
    use std::os::unix::fs::PermissionsExt;

    let path_dir = empty_dir();
    let project = tempdir().unwrap();
    let _env = EnvGuard::set_path(path_dir.path());
    std::fs::set_permissions(project.path(), std::fs::Permissions::from_mode(0o600)).unwrap();

    let enforced = std::fs::metadata(project.path().join(".")).is_err();
    if !enforced {
        std::fs::set_permissions(project.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
        eprintln!("unsearchable_project_dir_is_unclassified: NOT EXERCISED (privileged process)");
        return;
    }

    let exec = executor_in(project.path(), vec![mech("make-noop", &["make", "noop"])]);
    let r = run(&exec, "make-noop");
    // Restore so the temp dir can be cleaned up.
    std::fs::set_permissions(project.path(), std::fs::Permissions::from_mode(0o700)).unwrap();

    assert_eq!(r.outcome, ExecutionOutcome::Error);
    assert_eq!(
        r.classification,
        Some(ExecutionClassification::Unclassified)
    );
}

/// AC-9: the OS detail is preserved; output is empty.
#[test]
fn startup_failure_preserves_detail() {
    let path_dir = empty_dir();
    let project = tempdir().unwrap();
    let _env = EnvGuard::set_path(path_dir.path());
    let exec = executor_in(project.path(), vec![mech("make-noop", &["make", "noop"])]);

    let r = run(&exec, "make-noop");
    let detail = r.blocking_condition.expect("detail preserved");
    assert!(detail.contains("failed to start `make`"), "{detail}");
    assert!(detail.contains("os error"), "OS description kept: {detail}");
    assert!(r.captured_output.is_empty());
    assert!(r.exercised_behaviors.is_empty());
    assert_eq!(r.command_id, IdRef("make-noop".to_string()));
}

/// AC-27: two start failures with the same error kind and different detail text
/// get the same classification.
#[test]
fn classification_is_independent_of_blocking_condition_text() {
    let path_dir = empty_dir();
    let project = tempdir().unwrap();
    let _env = EnvGuard::set_path(path_dir.path());
    let exec = executor_in(
        project.path(),
        vec![
            mech("a", &["cargo", "build"]),
            mech("b", &["npm", "run", "x"]),
        ],
    );

    let a = run(&exec, "a");
    let b = run(&exec, "b");
    assert_ne!(a.blocking_condition, b.blocking_condition);
    assert_eq!(a.outcome, b.outcome);
    assert_eq!(a.classification, b.classification);
    assert_eq!(a.classification, Some(ExecutionClassification::InfraLevel));
}

/// AC-4: a non-zero exit is FAILED + UNCLASSIFIED, never APP_LEVEL.
#[test]
fn nonzero_exit_is_unclassified_never_app() {
    let project = tempdir().unwrap();
    let _env = EnvGuard::keep_path();
    for code in ["1", "2", "126", "127"] {
        let script = format!("exit {code}");
        let exec = executor_in(project.path(), vec![mech("x", &["sh", "-c", &script])]);
        let r = run(&exec, "x");
        assert_eq!(r.outcome, ExecutionOutcome::Failed, "exit {code}");
        assert_eq!(
            r.classification,
            Some(ExecutionClassification::Unclassified),
            "exit {code}"
        );
    }
}

/// AC-4: termination by signal is FAILED + UNCLASSIFIED.
#[cfg(unix)]
#[test]
fn signal_termination_is_unclassified_never_app() {
    let project = tempdir().unwrap();
    let _env = EnvGuard::keep_path();
    let exec = executor_in(project.path(), vec![mech("x", &["sh", "-c", "kill -9 $$"])]);
    let r = run(&exec, "x");
    assert_eq!(r.outcome, ExecutionOutcome::Failed);
    assert_eq!(
        r.classification,
        Some(ExecutionClassification::Unclassified)
    );
}

/// AC-3: adversarial stderr never changes the classification.
#[test]
fn failed_with_adversarial_stderr_is_unclassified() {
    let project = tempdir().unwrap();
    let _env = EnvGuard::keep_path();
    for text in [
        "command not found",
        "No such file or directory",
        "permission denied",
        "ECONNREFUSED",
        "npm ERR! missing script",
        "AssertionError: app bug",
        "APP_LEVEL INFRA_LEVEL",
    ] {
        let script = format!("echo '{text}' >&2; exit 3");
        let exec = executor_in(project.path(), vec![mech("x", &["sh", "-c", &script])]);
        let r = run(&exec, "x");
        assert_eq!(r.outcome, ExecutionOutcome::Failed, "{text}");
        assert_eq!(
            r.classification,
            Some(ExecutionClassification::Unclassified),
            "{text}"
        );
        assert!(r.captured_output.contains(text.split(' ').next().unwrap()));
    }
}

/// AC-19: TIMEOUT and NOT_RUN carry no classification.
#[test]
fn timeout_and_not_run_have_no_classification() {
    let project = tempdir().unwrap();
    let _env = EnvGuard::keep_path();

    let mut watcher = mech("watch", &["sleep", "60"]);
    watcher.non_terminating_reason = Some("watcher".to_string());
    let registry = MechanismRegistry::from_entries(vec![mech("hang", &["sleep", "60"]), watcher]);
    let exec =
        ProjectCommandExecutor::with_bound(registry, project.path(), Duration::from_millis(300));

    let t = run(&exec, "hang");
    assert_eq!(t.outcome, ExecutionOutcome::Timeout);
    assert_eq!(t.classification, None);

    let n = run(&exec, "watch");
    assert_eq!(n.outcome, ExecutionOutcome::NotRun);
    assert_eq!(n.classification, None);
}

/// A passing run carries no classification.
#[test]
fn passed_has_no_classification() {
    let project = tempdir().unwrap();
    let _env = EnvGuard::keep_path();
    let exec = executor_in(project.path(), vec![mech("ok", &["sh", "-c", "exit 0"])]);
    let r = run(&exec, "ok");
    assert_eq!(r.outcome, ExecutionOutcome::Passed);
    assert_eq!(r.classification, None);
}

/// AC-8: an unknown id is still `Err(NotFound)`.
#[test]
fn unknown_id_still_not_found() {
    let project = tempdir().unwrap();
    let _env = EnvGuard::keep_path();
    let exec = executor_in(project.path(), vec![mech("known", &["sh", "-c", "exit 0"])]);
    let err = exec
        .run_existing(&IdRef("unknown".to_string()))
        .unwrap_err();
    assert_eq!(err, CapabilityError::NotFound("unknown".to_string()));
}

/// AC-8: an empty argument vector is still `Err(Failed)` (registry integrity).
#[test]
fn empty_argv_stays_err_failed() {
    let project = tempdir().unwrap();
    let _env = EnvGuard::keep_path();
    let exec = executor_in(project.path(), vec![mech("empty", &[])]);
    let err = exec.run_existing(&IdRef("empty".to_string())).unwrap_err();
    assert_eq!(
        err,
        CapabilityError::Failed("empty argument vector".to_string())
    );
}

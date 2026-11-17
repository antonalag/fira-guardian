//! VerificationMechanism registry + `CommandExecutor` (EXEC-1, §6/§17).
//!
//! The executor runs **only** mechanisms discovered in the project
//! (`declared_by_project=true`), looked up by `command_id`. An id not in the
//! registry is rejected (`CapabilityError::NotFound`) — the runtime enforcement
//! of EXEC-1. There is no arbitrary-shell API: execution uses
//! `std::process::Command` with the discovered **explicit argument vector**, no
//! shell interpolation, and no network.
//!
//! The executor produces a **raw** `ExecutionResult` (outcome + captured output).
//! The §6 outcome→epistemic/gate interpretation is deliberately **not** done
//! here (CORE `gate_eval`). The `(outcome, classification)` pair is, however,
//! derived by the pure CORE function [`classify_run`] from normalized
//! [`RunFacts`]: this layer only observes the operating system (exit status,
//! `io::ErrorKind`, access to the project directory, launcher name) and
//! translates it into those neutral facts. It never decides a classification by
//! itself, and never reads captured output or error text to do so.
//!
//! A failure to *start* a declared mechanism is a result (`Ok(ERROR)` with the
//! operating-system detail preserved in `blocking_condition`), not an `Err`.
//! An unknown id and a registry integrity error remain `Err`.

use std::collections::BTreeSet;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use fira_core::execution::{ExecutionResult, VerificationMechanism};
use fira_core::failure_classification::{
    classify_run, LauncherProvenance, RunFacts, StartFailure, StartFailureCause,
};
use fira_core::interfaces::{CapabilityError, CommandExecutor};
use fira_core::model::{Capability, IdRef};

use crate::discovery::{discover, DiscoveredMechanism};

/// The MVP execution **safety bound** (Task 14, design §2.2/§5).
///
/// This is a runtime safety ceiling that prevents a non-terminating or hung
/// verification mechanism from blocking the audit indefinitely — **not** an
/// expected project test duration. The anchor from the first real audit:
/// `durable-agents`' real suite finishes in ~25 s, so 300 s tolerates a far
/// slower-but-legitimate suite/build while still converting an indefinite hang
/// into a bounded, honest `TIMEOUT`. On exceeding it, the runtime terminates the
/// process and produces `ExecutionOutcome::TIMEOUT` (§6) — never PASS (C5).
pub const DEFAULT_EXECUTION_BOUND: Duration = Duration::from_secs(300);

/// How long to wait after requesting termination before giving up on a clean
/// join of the output-reader threads (best-effort partial output, design §2.3).
const TERMINATION_JOIN_GRACE: Duration = Duration::from_secs(2);

/// The discovered registry: project-declared mechanisms + their argv. The only
/// commands the executor will ever run.
#[derive(Debug, Clone, Default)]
pub struct MechanismRegistry {
    entries: Vec<DiscoveredMechanism>,
}

impl MechanismRegistry {
    /// Discover mechanisms under `project_root`.
    pub fn discover(project_root: &std::path::Path) -> Self {
        MechanismRegistry {
            entries: discover(project_root),
        }
    }

    /// Build from an explicit list (used in tests).
    pub fn from_entries(entries: Vec<DiscoveredMechanism>) -> Self {
        MechanismRegistry { entries }
    }

    /// The S12 mechanism records (never synthesized).
    pub fn mechanisms(&self) -> Vec<VerificationMechanism> {
        self.entries.iter().map(|e| e.mechanism.clone()).collect()
    }

    fn find(&self, command_id: &IdRef) -> Option<&DiscoveredMechanism> {
        self.entries.iter().find(|e| &e.mechanism.id == command_id)
    }
}

/// Executes existing, project-declared mechanisms (EXECUTE_EXISTING; EXEC-1).
///
/// Execution is **bounded** (Task 14): a mechanism that does not terminate within
/// `bound` is actively terminated and reported as `TIMEOUT`. The child's stdin is
/// isolated (never inherits the audit process's stdin) so an interactive command
/// cannot hang the audit.
#[derive(Debug, Clone)]
pub struct ProjectCommandExecutor {
    registry: MechanismRegistry,
    cwd: PathBuf,
    /// The per-mechanism execution safety bound (Task 14).
    bound: Duration,
}

impl ProjectCommandExecutor {
    /// Build an executor whose working directory is the project root, using the
    /// MVP execution safety bound ([`DEFAULT_EXECUTION_BOUND`]).
    pub fn new(registry: MechanismRegistry, project_root: impl Into<PathBuf>) -> Self {
        Self::with_bound(registry, project_root, DEFAULT_EXECUTION_BOUND)
    }

    /// Build an executor with an explicit execution bound. Used by tests to set a
    /// sub-second bound; production uses [`ProjectCommandExecutor::new`].
    pub fn with_bound(
        registry: MechanismRegistry,
        project_root: impl Into<PathBuf>,
        bound: Duration,
    ) -> Self {
        ProjectCommandExecutor {
            registry,
            cwd: project_root.into(),
            bound,
        }
    }

    /// The configured execution safety bound.
    pub fn bound(&self) -> Duration {
        self.bound
    }
}

impl CommandExecutor for ProjectCommandExecutor {
    fn run_existing(&self, command_id: &IdRef) -> Result<ExecutionResult, CapabilityError> {
        // EXEC-1: reject any id not in the discovered registry.
        let entry = self
            .registry
            .find(command_id)
            .ok_or_else(|| CapabilityError::NotFound(command_id.0.clone()))?;

        // Task 14: a mechanism whose declared command is structurally
        // non-terminating (watcher / dev server / REPL) is NOT executed for
        // verification. It remains project evidence (still in the registry); we
        // return a NOT_RUN result stating it was not executed and why. No command
        // is synthesized or rewritten (EXEC-1); nothing is marked PASS (C5).
        if let Some(reason) = &entry.non_terminating_reason {
            return Ok(build_result(
                entry,
                &RunFacts::NotExecuted,
                Some(format!(
                    "declared command is a non-terminating {reason}; not executed for \
                     verification (would not terminate on its own)"
                )),
                String::new(),
            ));
        }

        // Explicit argv; no shell. argv[0] is the program. An empty vector is a
        // registry integrity error and stays an `Err` (not a run result).
        let (program, args) = entry
            .argv
            .split_first()
            .ok_or_else(|| CapabilityError::Failed("empty argument vector".to_string()))?;

        // Who chose the program (exact match against FIRA's own literals).
        let launcher = launcher_provenance(program);

        // Access to the audited project directory must be verifiable before
        // anything is launched. If it is not, nothing is started and the result
        // carries no infrastructure attribution.
        let accessible_before = project_dir_accessible(&self.cwd);
        if !accessible_before {
            return Ok(build_result(
                entry,
                &RunFacts::StartFailed(StartFailure {
                    cause: StartFailureCause::ProjectPathUnavailable,
                    launcher,
                }),
                Some(
                    "audited project path is not an accessible directory (access could not \
                     be verified); the mechanism was not started"
                        .to_string(),
                ),
                String::new(),
            ));
        }

        // Spawn (not `output()`): we must be able to interrupt a still-running
        // child, which `output()` cannot do (it blocks until EOF/exit).
        // stdin is Stdio::null() — a verification run is non-interactive, so a
        // command that reads stdin sees EOF and can never block the audit on
        // inherited input (Task 14). stdout/stderr are piped so we can drain and
        // capture them (incl. partial output on timeout).
        let mut cmd = Command::new(program);
        cmd.args(args)
            .current_dir(&self.cwd)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        // Put the child in its own process group so termination can target the
        // group leader (design §2.5). No new dependency: std sets the group at
        // spawn; the actual signal is Child::kill (direct child) — see
        // terminate_child + the documented subtree limitation.
        set_own_process_group(&mut cmd);

        let mut child = match cmd.spawn() {
            Ok(child) => child,
            Err(e) => {
                // The process did not start: a result, not an `Err`. The OS
                // `ErrorKind` is normalized together with a second check of the
                // project directory; the OS text is kept only as detail.
                let accessible_after = project_dir_accessible(&self.cwd);
                let cause = start_failure_cause(e.kind(), accessible_before, accessible_after);
                return Ok(build_result(
                    entry,
                    &RunFacts::StartFailed(StartFailure { cause, launcher }),
                    Some(format!("failed to start `{program}`: {e}")),
                    String::new(),
                ));
            }
        };

        // Drain stdout/stderr on their own threads so a chatty child cannot
        // deadlock against a full pipe while we wait on the bound.
        let stdout_rx = spawn_reader(child.stdout.take());
        let stderr_rx = spawn_reader(child.stderr.take());

        // Enforce the bound: poll for exit until the deadline, then terminate the
        // live child. This genuinely interrupts a still-running process — it is
        // not an elapsed-time check after a completed `output()`.
        let outcome = wait_with_bound(&mut child, self.bound);

        // Collect whatever output was captured (best-effort on timeout).
        let mut captured = String::new();
        captured.push_str(&collect_reader(stdout_rx));
        let err = collect_reader(stderr_rx);
        if !err.is_empty() {
            captured.push_str(&err);
        }

        let (facts, blocking_condition) = match outcome {
            WaitOutcome::Exited(success) => (RunFacts::Completed { success }, None),
            WaitOutcome::TimedOut { terminated } => {
                let detail = if terminated {
                    format!(
                        "exceeded {}s execution safety bound; process terminated",
                        self.bound.as_secs()
                    )
                } else {
                    format!(
                        "exceeded {}s execution safety bound; termination was attempted but \
                         the process could not be confirmed terminated",
                        self.bound.as_secs()
                    )
                };
                // Fail-safe: a timeout is UNKNOWN downstream, never PASS (C5).
                (RunFacts::TimedOut, Some(detail))
            }
        };

        Ok(build_result(entry, &facts, blocking_condition, captured))
    }

    fn capabilities(&self) -> BTreeSet<Capability> {
        // MVP boundary: READ + EXECUTE_EXISTING. NETWORK is never included.
        let mut caps = BTreeSet::new();
        caps.insert(Capability::Read);
        caps.insert(Capability::ExecuteExisting);
        caps
    }

    fn discover_mechanisms(&self) -> Result<Vec<VerificationMechanism>, CapabilityError> {
        Ok(self.registry.mechanisms())
    }
}

// ---------------------------------------------------------------------------
// Task 14 execution-bound helpers (RUNTIME-local; no new dependency, std only).
// ---------------------------------------------------------------------------

/// The result of waiting on a bounded child.
enum WaitOutcome {
    /// The child exited on its own; `bool` is `status.success()`.
    Exited(bool),
    /// The bound was exceeded; `terminated` records whether we confirmed the
    /// child was killed (best-effort — see §2.6). Either way the outcome is a
    /// fail-safe `TIMEOUT`, never PASS.
    TimedOut { terminated: bool },
}

/// Poll `child` for exit until `bound` elapses; on timeout, actively terminate
/// it (real interrupt, not a post-hoc elapsed check) and report whether
/// termination was confirmed.
fn wait_with_bound(child: &mut Child, bound: Duration) -> WaitOutcome {
    use std::time::Instant;
    let deadline = Instant::now() + bound;
    // Coarse poll; small enough to be responsive, large enough to be cheap.
    let poll = Duration::from_millis(25);
    loop {
        match child.try_wait() {
            Ok(Some(status)) => return WaitOutcome::Exited(status.success()),
            Ok(None) => {
                if Instant::now() >= deadline {
                    let terminated = terminate_child(child);
                    // Reap so the OS does not leave a zombie; ignore the status.
                    let _ = child.wait();
                    return WaitOutcome::TimedOut { terminated };
                }
                thread::sleep(poll);
            }
            // try_wait errored: we cannot determine state. Attempt termination
            // and report it as a (non-PASS) timeout-equivalent so we never hang
            // or claim success.
            Err(_) => {
                let terminated = terminate_child(child);
                let _ = child.wait();
                return WaitOutcome::TimedOut { terminated };
            }
        }
    }
}

/// Terminate a still-running child. Returns `true` if the kill call succeeded.
///
/// Uses `std::process::Child::kill()` (portable, no dependency). On Unix the
/// child was placed in its own process group at spawn
/// ([`set_own_process_group`]); `kill()` signals the direct child. Full subtree
/// teardown of grandchildren (e.g. test-runner workers) without a `libc`/
/// platform crate is **not** performed here — a documented MVP limitation
/// (design §2.5). The classifier (discovery) prevents the common
/// non-terminating cases from ever being executed, so this bound is the backstop
/// for a one-shot command that unexpectedly hangs.
fn terminate_child(child: &mut Child) -> bool {
    child.kill().is_ok()
}

/// Spawn a thread that reads a piped handle to EOF. Returns a receiver that
/// yields the captured bytes as a lossy UTF-8 string once the pipe closes.
fn spawn_reader<R: Read + Send + 'static>(handle: Option<R>) -> Option<mpsc::Receiver<String>> {
    let mut handle = handle?;
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        let mut buf = Vec::new();
        // Best-effort: on a killed child the read returns what was buffered.
        let _ = handle.read_to_end(&mut buf);
        let _ = tx.send(String::from_utf8_lossy(&buf).into_owned());
    });
    Some(rx)
}

/// Collect a reader's captured output, bounded by a short grace so a stuck
/// reader thread cannot re-hang the audit after termination (design §2.3).
fn collect_reader(rx: Option<mpsc::Receiver<String>>) -> String {
    match rx {
        Some(rx) => rx.recv_timeout(TERMINATION_JOIN_GRACE).unwrap_or_default(),
        None => String::new(),
    }
}

/// Put the child into its own process group so termination can target the group
/// leader (design §2.5). Unix-only; a no-op elsewhere. std-only: this sets the
/// group at spawn via `pre_exec`-free `CommandExt::process_group`.
#[cfg(unix)]
fn set_own_process_group(cmd: &mut Command) {
    use std::os::unix::process::CommandExt;
    // 0 ⇒ the child becomes the leader of a new process group equal to its PID.
    cmd.process_group(0);
}

/// Non-Unix: no process-group handling available without a platform crate;
/// termination falls back to the direct child (documented limitation).
#[cfg(not(unix))]
fn set_own_process_group(_cmd: &mut Command) {}

// ---------------------------------------------------------------------------
// Failure-classification helpers (RUNTIME observes; CORE decides).
// ---------------------------------------------------------------------------

/// Launchers that `discovery.rs` writes as literals for `argv[0]` (not text
/// coming from the audited project). Kept in sync with discovery by the unit
/// test `fira_selected_launchers_match_discovery`.
const FIRA_SELECTED_LAUNCHERS: [&str; 3] = ["cargo", "npm", "make"];

/// `SelectedByFira` only when `program` is exactly one of FIRA's own launcher
/// literals. Paths (`./cargo`, `/usr/bin/cargo`), other cases and affixed names
/// are not established.
fn launcher_provenance(program: &str) -> LauncherProvenance {
    if FIRA_SELECTED_LAUNCHERS.contains(&program) {
        LauncherProvenance::SelectedByFira
    } else {
        LauncherProvenance::NotEstablished
    }
}

/// Whether access to the audited project directory can be verified: resolving
/// `dir/.` must succeed and yield a directory. Resolving `.` requires search
/// permission on `dir`. The directory is not enumerated; `PATH` and executable
/// bits are not inspected.
fn project_dir_accessible(dir: &Path) -> bool {
    std::fs::metadata(dir.join("."))
        .map(|m| m.is_dir())
        .unwrap_or(false)
}

/// Normalize an OS error kind plus the two project-directory checks (before and
/// after the start attempt) into a [`StartFailureCause`]. Any unverifiable
/// access wins over the error kind.
fn start_failure_cause(
    kind: std::io::ErrorKind,
    accessible_before: bool,
    accessible_after: bool,
) -> StartFailureCause {
    if !(accessible_before && accessible_after) {
        return StartFailureCause::ProjectPathUnavailable;
    }
    match kind {
        std::io::ErrorKind::NotFound => StartFailureCause::ProgramNotFound,
        std::io::ErrorKind::PermissionDenied => StartFailureCause::ProgramNotExecutable,
        _ => StartFailureCause::Other,
    }
}

/// The single constructor of an `ExecutionResult`: `(outcome, classification)`
/// come only from [`classify_run`]; `blocking_condition` and `captured_output`
/// are carried as detail and never feed the classification.
fn build_result(
    entry: &DiscoveredMechanism,
    facts: &RunFacts,
    blocking_condition: Option<String>,
    captured_output: String,
) -> ExecutionResult {
    let classified = classify_run(facts);
    ExecutionResult {
        command_id: entry.mechanism.id.clone(),
        command: entry.mechanism.command.clone(),
        outcome: classified.outcome,
        classification: classified.classification,
        blocking_condition,
        captured_output,
        exercised_behaviors: Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::{
        launcher_provenance, project_dir_accessible, start_failure_cause, FIRA_SELECTED_LAUNCHERS,
    };
    use fira_core::failure_classification::{LauncherProvenance, StartFailureCause};
    use std::collections::BTreeSet;
    use std::io::ErrorKind;
    use tempfile::tempdir;

    /// AC-6: the allowlist equals the set of `argv[0]` literals discovery emits.
    #[test]
    fn fira_selected_launchers_match_discovery() {
        let dir = tempdir().unwrap();
        std::fs::write(
            dir.path().join("Cargo.toml"),
            "[package]\nname = \"demo\"\nversion = \"0.1.0\"\n",
        )
        .unwrap();
        std::fs::write(
            dir.path().join("package.json"),
            "{\"scripts\":{\"test\":\"echo ok\"}}",
        )
        .unwrap();
        std::fs::write(dir.path().join("Makefile"), "build:\n\ttrue\n").unwrap();

        let launchers: BTreeSet<String> = crate::discovery::discover(dir.path())
            .iter()
            .map(|m| m.argv[0].clone())
            .collect();
        let allowlist: BTreeSet<String> = FIRA_SELECTED_LAUNCHERS
            .iter()
            .map(|s| s.to_string())
            .collect();
        assert_eq!(
            launchers, allowlist,
            "discovery launchers and the FIRA allowlist must be reviewed together"
        );
    }

    /// AC-29: INFRA-capable causes only with both checks true and
    /// NotFound/PermissionDenied; any false check is ProjectPathUnavailable.
    #[test]
    fn start_failure_cause_mapping() {
        let kinds = [
            ErrorKind::NotFound,
            ErrorKind::PermissionDenied,
            ErrorKind::InvalidInput,
            ErrorKind::Other,
        ];
        for kind in kinds {
            for (before, after) in [(true, true), (true, false), (false, true), (false, false)] {
                let got = start_failure_cause(kind, before, after);
                let expected = if !(before && after) {
                    StartFailureCause::ProjectPathUnavailable
                } else {
                    match kind {
                        ErrorKind::NotFound => StartFailureCause::ProgramNotFound,
                        ErrorKind::PermissionDenied => StartFailureCause::ProgramNotExecutable,
                        _ => StartFailureCause::Other,
                    }
                };
                assert_eq!(got, expected, "{kind:?} before={before} after={after}");
            }
        }
    }

    #[test]
    fn launcher_provenance_is_exact_match() {
        for ok in ["cargo", "npm", "make"] {
            assert_eq!(launcher_provenance(ok), LauncherProvenance::SelectedByFira);
        }
        for bad in [
            "./cargo",
            "/usr/bin/cargo",
            "Cargo",
            "cargo ",
            "xcargo",
            "cargo-build",
            "",
            "sh",
        ] {
            assert_eq!(
                launcher_provenance(bad),
                LauncherProvenance::NotEstablished,
                "{bad:?}"
            );
        }
    }

    #[test]
    fn project_dir_accessible_existing_dir() {
        let dir = tempdir().unwrap();
        assert!(project_dir_accessible(dir.path()));
    }

    #[test]
    fn project_dir_accessible_missing_dir() {
        let dir = tempdir().unwrap();
        assert!(!project_dir_accessible(&dir.path().join("does-not-exist")));
    }

    #[test]
    fn project_dir_accessible_regular_file() {
        let dir = tempdir().unwrap();
        let file = dir.path().join("file");
        std::fs::write(&file, "x").unwrap();
        assert!(!project_dir_accessible(&file));
    }
}

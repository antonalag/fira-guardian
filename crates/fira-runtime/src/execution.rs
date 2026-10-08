//! VerificationMechanism registry + `CommandExecutor` (EXEC-1, §6/§17).
//!
//! The executor runs **only** mechanisms discovered in the project
//! (`declared_by_project=true`), looked up by `command_id`. An id not in the
//! registry is rejected (`CapabilityError::NotFound`) — the runtime enforcement
//! of EXEC-1. There is no arbitrary-shell API: execution uses
//! `std::process::Command` with the discovered **explicit argument vector**, no
//! shell interpolation, and no network.
//!
//! Task 10 produces a **raw** `ExecutionResult` (outcome + captured output). The
//! §6 outcome→epistemic/gate interpretation is deliberately **not** done here
//! (deferred to later CORE orchestration).

use std::collections::BTreeSet;
use std::io::Read;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use fira_core::execution::{ExecutionResult, VerificationMechanism};
use fira_core::interfaces::{CapabilityError, CommandExecutor};
use fira_core::model::{Capability, ExecutionOutcome, IdRef};

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
            return Ok(ExecutionResult {
                command_id: entry.mechanism.id.clone(),
                command: entry.mechanism.command.clone(),
                outcome: ExecutionOutcome::NotRun,
                classification: None,
                blocking_condition: Some(format!(
                    "declared command is a non-terminating {reason}; not executed for \
                     verification (would not terminate on its own)"
                )),
                captured_output: String::new(),
                exercised_behaviors: Vec::new(),
            });
        }

        // Explicit argv; no shell. argv[0] is the program.
        let (program, args) = entry
            .argv
            .split_first()
            .ok_or_else(|| CapabilityError::Failed("empty argument vector".to_string()))?;

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

        let mut child = cmd
            .spawn()
            .map_err(|e| CapabilityError::Failed(format!("spawn failed: {e}")))?;

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

        let (outcome, blocking_condition) = match outcome {
            WaitOutcome::Exited(success) => (
                if success {
                    ExecutionOutcome::Passed
                } else {
                    ExecutionOutcome::Failed
                },
                None,
            ),
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
                (ExecutionOutcome::Timeout, Some(detail))
            }
        };

        Ok(ExecutionResult {
            command_id: entry.mechanism.id.clone(),
            command: entry.mechanism.command.clone(),
            outcome,
            classification: None,
            blocking_condition,
            captured_output: captured,
            exercised_behaviors: Vec::new(),
        })
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

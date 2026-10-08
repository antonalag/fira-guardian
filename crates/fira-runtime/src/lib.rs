//! # FIRA Guardian — RUNTIME
//!
//! Executes audits under enforced capabilities. This layer owns:
//! - capability enforcement of `{READ, EXECUTE_EXISTING}` against the audited
//!   project tree; no `WRITE_PROJECT`/`CREATE_FILE`/`DELETE_FILE`; `NETWORK`
//!   denied (CAP-1);
//! - the VerificationMechanism registry and `run_existing(command_id)` that
//!   rejects any command not discovered in the project (EXEC-1);
//! - production of raw `ExecutionResult` (the §6 outcome → epistemic/gate mapping
//!   is deferred to later CORE orchestration);
//! - output path containment (WS-1/P-1) via [`capability::OutputSink`].
//!
//! RUNTIME is the *primary* enforcement mechanism for CAP-1/EXEC-1 (together with
//! the architectural/dependency boundary — ADR-002). External audit-workspace
//! persistence (WS-1 report saving, [`persistence::WorkspaceSink`]) is Task 11;
//! it is bytes-in and bound to a workspace base distinct from the project tree.

pub mod capability;
pub mod discovery;
pub mod evidence;
pub mod execution;
pub mod persistence;

pub use capability::{FsRepositoryReader, OutputPathError, OutputSink};
pub use discovery::{detect_signals, discover, DiscoveredMechanism};
pub use evidence::EvidenceCollectorImpl;
pub use execution::{MechanismRegistry, ProjectCommandExecutor};
pub use persistence::{FsyncObserver, OverwritePolicy, RealFsync, WorkspaceError, WorkspaceSink};

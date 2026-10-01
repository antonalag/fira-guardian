//! # FIRA Guardian — RUNTIME
//!
//! Executes audits under enforced capabilities. This layer will own:
//! - capability enforcement of `{READ, EXECUTE_EXISTING}` against the audited
//!   project tree; no `WRITE_PROJECT`/`CREATE_FILE`/`DELETE_FILE`; `NETWORK`
//!   denied (CAP-1);
//! - the VerificationMechanism registry and `run_existing(command_id)` that
//!   rejects any command not discovered in the project (EXEC-1);
//! - production of `ExecutionResult` and the outcome → epistemic/gate mapping;
//! - persistence of FIRA's own report to the **external** audit workspace
//!   `/fira-workspace/<audit-id>/` (WS-1 / P-1 / VR13).
//!
//! RUNTIME is the *primary* enforcement mechanism for WS-1 and EXEC-1 (together
//! with the architectural/dependency boundary). The source-scan test in Task 1
//! is only defense-in-depth, not the enforcement mechanism.
//!
//! ## Task 1 scope
//! Home only. No capability enforcement, process execution, or persistence is
//! implemented yet (Tasks 5 / 9 / 10 / 11).

/// Capability enforcement over the audited project tree. Implemented in Task 10.
pub mod capability {}

/// VerificationMechanism registry + `run_existing` (EXEC-1). Implemented in Task 5 / 10.
pub mod execution {}

/// External audit-workspace persistence (WS-1). Implemented in Task 11.
///
/// Persistence targets `/fira-workspace/<audit-id>/` and is a separate concern
/// from project capabilities: saving FIRA's report must never require or grant
/// a `WRITE` capability over the audited project tree.
pub mod persistence {}

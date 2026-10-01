//! S5 ExecutionResult + S12 VerificationMechanism — **PROVISIONAL Task-4 seam**.
//!
//! # Ownership
//!
//! S5 and S12 are owned by **Task 5**. This module exists in Task 4 only to
//! provide the minimal structural shape that a Task-4-owned type references: S11
//! AuditReport carries `verification_mechanisms: Vec<VerificationMechanism>`
//! (the discovered registry). Without a concrete `VerificationMechanism` type,
//! `AuditReport` could not be typed or round-trip the Task 2 golden fixture.
//!
//! # Constraints on this seam (Task 4)
//!
//! - **Structural only.** Fields copied verbatim from the Task 2 S12 schema
//!   (`schemas/s12-verification-mechanism.schema.json`) / `schema-formalization.md`.
//! - **No Task-5 semantics.** No EXEC-1 enforcement, no registry/discovery
//!   behavior, no process execution, no VR12 logic (already enforced in Task 2),
//!   no methods beyond `derive`.
//! - **Minimal.** Only `VerificationMechanism` is referenced by a Task-4 owner,
//!   so only it is defined here. `ExecutionResult` (S5) is **not** referenced by
//!   any Task-4-owned type (S11 coverage refers to execution results only by id
//!   string), and is intentionally left for Task 5 to author.
//! - **Finalized by Task 5.** Task 5 must **reconcile** this provisional type
//!   rather than create a competing duplicate.
//!
//! If typing a Task-4 owner ever requires an actual S5/S12 behavior, that is
//! Task-5 work → STOP and report.

use serde::{Deserialize, Serialize};

use crate::model::{AlwaysTrue, IdRef, VerificationMechanismSource};

/// VerificationMechanism (S12). Provisional seam type (see module docs).
/// Referenced by S11 AuditReport's `verification_mechanisms` registry.
///
/// `declared_by_project` is fixed to `true` (S12/EXEC-1: a mechanism is never
/// synthesized); it is typed as [`AlwaysTrue`] to mirror the schema's
/// `const: true`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VerificationMechanism {
    pub id: IdRef,
    pub source: VerificationMechanismSource,
    /// file:line of declaration.
    pub source_locator: String,
    pub command: String,
    pub declared_by_project: AlwaysTrue,
}

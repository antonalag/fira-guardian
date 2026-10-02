//! S5 ExecutionResult + S12 VerificationMechanism — **FINAL (Task 5)**.
//!
//! # Ownership
//!
//! S5 and S12 are owned by **Task 5**. `VerificationMechanism` was first
//! introduced as a provisional structural seam in Task 4 (so S11 AuditReport
//! could type its `verification_mechanisms` registry); Task 5 **promotes it in
//! place** — same module, same public name, same fields — and adds `ExecutionResult`.
//! There is exactly one `VerificationMechanism` type in the crate; Task 4 owners
//! keep compiling unchanged.
//!
//! # Scope (Task 5)
//!
//! - **Data types only.** Fields copied verbatim from the Task 2 S5/S12 schemas
//!   (`schemas/s05-execution-result.schema.json`,
//!   `schemas/s12-verification-mechanism.schema.json`) / `schema-formalization.md`.
//! - **No EXEC-1 / VR12 logic.** The `command_id → declared_by_project=true`
//!   foreign-key resolution (VR12) is enforced by the Task 2 validator over JSON;
//!   it is not re-encoded as type logic here. `declared_by_project` is fixed to
//!   `true` via [`AlwaysTrue`], mirroring the schema's `const: true`.
//! - **No execution behavior.** Running mechanisms and discovering the registry
//!   are capabilities (`CommandExecutor`, declared abstractly in
//!   `crate::interfaces`); their implementation is a runtime/adapter concern
//!   (Task 9/10/11), not here. CORE performs no process, filesystem, or network
//!   I/O.

use serde::{Deserialize, Serialize};

use crate::model::{
    AlwaysTrue, ExecutionClassification, ExecutionOutcome, IdRef, VerificationMechanismSource,
};

/// VerificationMechanism (S12). Referenced by S11 AuditReport's
/// `verification_mechanisms` registry and by `ExecutionResult.command_id` (FK).
///
/// `declared_by_project` is fixed to `true` (S12/EXEC-1: a mechanism is never
/// synthesized); typed as [`AlwaysTrue`] to mirror the schema's `const: true`.
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

/// ExecutionResult (S5): the result of running an existing verification
/// mechanism.
///
/// `command_id` is a foreign key to [`VerificationMechanism::id`] (VR12,
/// schema/validator-enforced — not a type-level check here). "Command executed"
/// never auto-implies "behavior verified" (frozen contract §6); that mapping is
/// a later engine's concern.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExecutionResult {
    /// FK → VerificationMechanism.id (VR12).
    pub command_id: IdRef,
    pub command: String,
    pub outcome: ExecutionOutcome,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub classification: Option<ExecutionClassification>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub blocking_condition: Option<String>,
    pub captured_output: String,
    pub exercised_behaviors: Vec<String>,
}

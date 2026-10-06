//! S5 ExecutionResult + S12 VerificationMechanism.
//!
//! Data types only. The `command_id → declared_by_project=true` FK resolution
//! (VR12/EXEC-1) is validator-enforced; running mechanisms and discovering the
//! registry are capabilities (`crate::interfaces::CommandExecutor`) implemented
//! by the runtime.

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
/// "Command executed" never auto-implies "behavior verified" (§6); the
/// outcome→epistemic mapping is a separate engine's concern.
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

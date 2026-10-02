//! S4 EvidenceRef + SupportMapping — **FINAL (Task 5)**.
//!
//! # Ownership
//!
//! S4 is owned by **Task 5**. `SupportMapping` was first introduced as a
//! provisional structural seam in Task 4 (so S7 Finding and S8 Gate could be
//! typed); Task 5 **promotes it in place** — same module, same public name, same
//! fields — and adds the second S4 shape, `EvidenceRef`. There is exactly one
//! `SupportMapping` type in the crate; Task 4 owners keep compiling unchanged.
//!
//! # Scope (Task 5)
//!
//! - **Data types only.** Fields are copied verbatim from the Task 2 S4 schema
//!   (`schemas/s04-evidence-support-mapping.schema.json`) / `schema-formalization.md`.
//! - **No VR logic.** VR3 (`captured_output_ref` present iff `type=COMMAND`) and
//!   VR1 (sufficiency) are enforced by the Task 2 schema/validator over JSON;
//!   they are not re-encoded as type logic here.
//! - **No evidence-collection behavior.** Capturing evidence is a capability
//!   (`EvidenceCollector`, declared abstractly in `crate::interfaces`); its
//!   implementation is a runtime/adapter concern (Task 9/10/11), not here. CORE
//!   performs no filesystem, process, or network I/O.

use serde::{Deserialize, Serialize};

use crate::model::{EvidenceType, IdRef, Sufficiency};

/// EvidenceRef (S4): a reference to a piece of evidence plus its validity.
///
/// `captured_output_ref` is required iff `evidence_type == COMMAND` (VR3); that
/// conditional is enforced by the S4 JSON Schema, not at the Rust type level, so
/// here it is a plain optional field.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EvidenceRef {
    pub ref_id: IdRef,
    /// Serialized as `type` (a Rust keyword), hence the field rename.
    #[serde(rename = "type")]
    pub evidence_type: EvidenceType,
    /// file:line-range | test_id | config:key | command_id | doc:section.
    pub locator: String,
    pub valid: bool,
    /// command_id; required iff `evidence_type == COMMAND` (VR3, schema-enforced).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub captured_output_ref: Option<IdRef>,
}

/// SupportMapping (S4). Referenced by S7 Finding and S8 Gate.
///
/// `evidence_refs` is `>=1` per the schema (VR1); the non-empty check is owned by
/// the Task 2 VR validator, not enforced at the type level here.
/// `unverified_remainder` is required by the schema when `sufficiency` is not
/// SUFFICIENT (a conditional the schema owns); here it is a plain optional field.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SupportMapping {
    pub claim: String,
    pub evidence_refs: Vec<IdRef>,
    pub relevant_span: String,
    pub sufficiency: Sufficiency,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unverified_remainder: Option<String>,
}

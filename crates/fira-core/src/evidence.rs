//! S4 EvidenceRef + SupportMapping — **PROVISIONAL Task-4 seam**.
//!
//! # Ownership
//!
//! S4 is owned by **Task 5**. This module exists in Task 4 only to provide the
//! minimal structural shape that the Task-4-owned types reference: S7 Finding and
//! S8 Gate carry `support_mappings: Vec<SupportMapping>`. Without a concrete
//! `SupportMapping` type, those owners could not be typed or round-trip the Task 2
//! valid fixtures.
//!
//! # Constraints on this seam (Task 4)
//!
//! - **Structural only.** Fields are copied verbatim from the Task 2 S4 schema
//!   (`schemas/s04-evidence-support-mapping.schema.json`) / `schema-formalization.md`.
//! - **No Task-5 semantics.** No validation logic, no VR enforcement (VR1/VR3 are
//!   already enforced in Task 2 over JSON), no evidence-collection behavior, no
//!   methods beyond `derive`.
//! - **Minimal.** Only `SupportMapping` is required by Task-4 owners, so only it
//!   is defined here. `EvidenceRef` (the other S4 shape) is **not** referenced by
//!   any Task-4-owned type and is intentionally left for Task 5 to author, to
//!   avoid expanding the seam beyond need.
//! - **Finalized by Task 5.** When Task 5 implements S4, it must **reconcile**
//!   this provisional type (extend/relocate it) rather than create a competing
//!   duplicate.
//!
//! If typing a Task-4 owner ever requires more than this verbatim structural
//! shape (e.g. an actual S4 behavior), that is Task-5 work → STOP and report.

use serde::{Deserialize, Serialize};

use crate::model::{IdRef, Sufficiency};

/// SupportMapping (S4). Provisional seam type (see module docs). Referenced by
/// S7 Finding and S8 Gate.
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

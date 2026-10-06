//! S4 EvidenceRef + SupportMapping.
//!
//! Data types only. VR3 (`captured_output_ref` present iff `type=COMMAND`) and
//! VR1 (sufficiency) are schema/validator-enforced, not re-encoded as type logic;
//! capturing evidence is a capability (`crate::interfaces::EvidenceCollector`)
//! implemented by the runtime.

use serde::{Deserialize, Serialize};

use crate::model::{EvidenceType, IdRef, Sufficiency};

/// EvidenceRef (S4): a reference to a piece of evidence plus its validity.
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
/// `evidence_refs ≥1` (VR1) and the `unverified_remainder`-when-not-SUFFICIENT
/// conditional are schema-owned, so both are plain fields here.
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

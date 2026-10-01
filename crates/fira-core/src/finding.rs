//! S7 Finding + FailureScenario + Confidence fields.
//!
//! Verbatim translation of `schema-formalization.md` §S7. Task 4 types the data;
//! it does **not** compute confidence — `confidence` and `confidence_derivation`
//! are plain data fields whose derivation (the rubric, VR5) is Task 6. The
//! VR1/VR2/VR10 constraints are already enforced by the Task 2 schema/validator;
//! they are not re-implemented here.

use serde::{Deserialize, Serialize};

use crate::epistemic::EpistemicState;
use crate::evidence::SupportMapping;
use crate::model::{
    Confidence, FindingId, GateName, LifecycleStatus, Maturity, ReleaseImpact, RequirementSource,
    SecurityPropertySource, Severity,
};

/// FailureScenario (S7 nested): required for concurrency/durability/recovery
/// findings (VR2, enforced in the Task 2 validator). `anchor` is a
/// `file:line-range`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FailureScenario {
    pub anchor: String,
    pub trigger: String,
    pub consequence: String,
}

/// The `security_property` overlay on a Finding (S7): just a source tag.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FindingSecurityProperty {
    pub source: SecurityPropertySource,
}

/// Finding (S7). All fields verbatim from the schema.
///
/// `confidence` is DERIVED (CONF-1) and `confidence_derivation` records the
/// rubric branch that produced it — but Task 4 only stores these as data; the
/// rubric that computes them is Task 6.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Finding {
    /// RR-NNN stable id (pattern enforced by the S7 schema).
    pub id: FindingId,
    pub maturity: Maturity,
    pub title: String,
    pub severity: Severity,
    /// DERIVED (CONF-1), not model-set. Plain data in Task 4; rubric is Task 6.
    pub confidence: Confidence,
    /// Rubric branch that produced `confidence` (audit trail). Plain data here.
    pub confidence_derivation: String,
    pub category: String,
    pub requirement_source: RequirementSource,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub security_property: Option<FindingSecurityProperty>,
    pub gates_affected: Vec<GateName>,
    pub epistemic_state: EpistemicState,
    /// `>=1` per the schema (VR1); the non-empty / SUFFICIENT checks are owned by
    /// the Task 2 validator. `SupportMapping` is the provisional Task-5 seam.
    pub support_mappings: Vec<SupportMapping>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub failure_scenario: Option<FailureScenario>,
    pub impact: String,
    pub release_impact: ReleaseImpact,
    pub recommended_remediation: String,
    pub verification_criteria: String,
    pub lifecycle_status: LifecycleStatus,
    pub regression: bool,
}

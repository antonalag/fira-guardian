//! S10 TechnicalAssessment + HumanDecision.
//!
//! Verbatim translation of `schema-formalization.md` §S10. Task 4 types the data;
//! it does **not** compute the verdict — `computed_by_rule` is a plain data field
//! whose producer (the verdict engine, VR7) is Task 7. No gate normalization, no
//! blocking/risk computation here.
//!
//! VR6 reflection: FIRA sets only {NOT_APPLICABLE, PENDING}. This is mirrored by
//! two types — [`HumanDecision`] (full external enum) and [`HumanDecisionFira`]
//! (restricted) — matching the Task 2 schema's `HumanDecision` /
//! `HumanDecisionFira` split. The canonical report (S11) uses the FIRA-produced
//! variant.

use serde::{Deserialize, Serialize};

use crate::model::{
    FindingId, GateName, HumanDecisionFiraState, HumanDecisionState, TechnicalAssessmentResult,
};

/// TechnicalAssessment (S10): FIRA-owned readiness result plus the blocking/risk
/// sets. `computed_by_rule` names the verdict rule that produced it — Task 4
/// stores it as data; the verdict engine is Task 7.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TechnicalAssessment {
    pub result: TechnicalAssessmentResult,
    pub blocking_findings: Vec<FindingId>,
    pub blocking_gates: Vec<GateName>,
    pub risk_findings: Vec<FindingId>,
    pub risk_gates: Vec<GateName>,
    pub computed_by_rule: String,
}

/// External HumanDecision (S10): the full `{NOT_APPLICABLE, PENDING, ACCEPTED,
/// REJECTED}` enum. This is **not** what FIRA emits; see [`HumanDecisionFira`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HumanDecision {
    pub state: HumanDecisionState,
}

/// FIRA-produced HumanDecision (S10 / VR6): the `state` is restricted to
/// {NOT_APPLICABLE, PENDING}. Used by the canonical [`AuditReport`](crate::report).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HumanDecisionFira {
    pub state: HumanDecisionFiraState,
}

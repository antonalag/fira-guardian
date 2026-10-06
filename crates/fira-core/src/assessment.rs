//! S10 TechnicalAssessment + HumanDecision.
//!
//! HumanDecision is split into two types so VR6 is unrepresentable to violate:
//! [`HumanDecision`] carries the full external enum, while [`HumanDecisionFira`]
//! is restricted to the {NOT_APPLICABLE, PENDING} values FIRA may set. The
//! canonical report (S11) uses the FIRA-produced variant. The verdict engine
//! that computes the assessment lives in `crate::verdict`.

use serde::{Deserialize, Serialize};

use crate::model::{
    FindingId, GateName, HumanDecisionFiraState, HumanDecisionState, TechnicalAssessmentResult,
};

/// TechnicalAssessment (S10): FIRA-owned readiness result plus the blocking/risk
/// sets. `computed_by_rule` names the verdict rule that produced it.
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

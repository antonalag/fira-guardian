//! S8 Gate.
//!
//! Verbatim translation of `schema-formalization.md` §S8. The VR4 (N/A ⟺
//! not_applicable) and VR8 (gate ↔ coverage) rules are enforced in the Task 2
//! schema/validator, not at the type level. Task 4 types the data only.

use serde::{Deserialize, Serialize};

use crate::evidence::SupportMapping;
use crate::model::{FindingId, GateCause, GateName, GateState, RequirementLevel};

/// Gate (S8).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Gate {
    pub name: GateName,
    pub requirement_level: RequirementLevel,
    pub state: GateState,
    pub rationale: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cause: Option<GateCause>,
    /// `SupportMapping` is the provisional Task-5 seam type.
    pub support_mappings: Vec<SupportMapping>,
    pub supporting_findings: Vec<FindingId>,
}

//! S9 CoverageStatement.
//!
//! Verbatim translation of `schema-formalization.md` §S9. The VR8 (every gate in
//! ≥1 coverage entry) and VR9 (no correct/safe language) rules are enforced in
//! the Task 2 validator, not at the type level. Task 4 types the data only.

use serde::{Deserialize, Serialize};

use crate::model::{CoverageResult, Depth, ExecutionOutcome, IdRef};

/// An audited area entry (S9). NO_ISSUE_FOUND always carries depth and never
/// implies proof of correctness (C11).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AuditedArea {
    pub area: String,
    pub depth: Depth,
    pub result: CoverageResult,
}

/// A skipped area entry (S9).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SkippedArea {
    pub area: String,
    pub reason: String,
}

/// A blocked area entry (S9). References an execution result by id.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BlockedArea {
    pub area: String,
    pub blocking_condition: String,
    pub execution_result_ref: IdRef,
}

/// An executed-mechanism entry (S9): a command id and its outcome.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExecutedMechanism {
    pub command_id: IdRef,
    pub outcome: ExecutionOutcome,
}

/// CoverageStatement (S9).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CoverageStatement {
    pub audited_areas: Vec<AuditedArea>,
    pub skipped_areas: Vec<SkippedArea>,
    pub blocked_areas: Vec<BlockedArea>,
    pub executed_mechanisms: Vec<ExecutedMechanism>,
    pub known_limitations: Vec<String>,
    pub assumptions: Vec<String>,
    pub unanchored_hypotheses: Vec<String>,
}

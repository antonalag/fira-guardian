//! S11 AuditReport (canonical machine report).
//!
//! `applied_profile` is an S11-local snapshot shape, deliberately distinct from
//! the full S2 `AuditProfile` type (POLICY): the report records the profile as
//! applied, not the profile definition.

use serde::{Deserialize, Serialize};

use crate::assessment::{HumanDecisionFira, TechnicalAssessment};
use crate::coverage::CoverageStatement;
use crate::execution::VerificationMechanism;
use crate::finding::Finding;
use crate::gate::Gate;
use crate::model::{DivergenceType, FindingId, GateName, IdRef, ProfileId, RequirementLevel};
use crate::request::AuditRequest;

/// A gate entry inside the S11 `applied_profile` snapshot. S11-local; mirrors the
/// profile's `{gate, requirement_level, na_justification?}` shape, not the full
/// S2 `AuditProfile`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AppliedProfileGate {
    pub gate: GateName,
    pub requirement_level: RequirementLevel,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub na_justification: Option<String>,
}

/// An N/A override recorded in the S11 `applied_profile` snapshot. `by` is fixed
/// to `human` by the schema (N/A only via profile / human override).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NaOverride {
    pub gate: GateName,
    pub by: NaOverrideBy,
}

/// The `by` tag of an [`NaOverride`]: the schema fixes this to `"human"`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum NaOverrideBy {
    #[serde(rename = "human")]
    Human,
}

/// The applied-profile snapshot recorded in S11 AuditReport. S11-local (not the
/// S2 `AuditProfile`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AppliedProfile {
    pub profile_id: ProfileId,
    pub version: String,
    pub gates: Vec<AppliedProfileGate>,
    pub na_overrides: Vec<NaOverride>,
}

/// A DivergenceRecord (S11; frozen contract §12). Immutable blind-vs-narrative
/// contrast produced at P7.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DivergenceRecord {
    pub blind_observation_ref: IdRef,
    pub narrative_claim_ref: IdRef,
    pub divergence_type: DivergenceType,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resulting_finding: Option<FindingId>,
}

/// AuditReport (S11): the canonical machine report.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AuditReport {
    pub schema_version: String,
    pub audit_id: String,
    pub created_at: String,
    pub request: AuditRequest,
    pub applied_profile: AppliedProfile,
    pub system_model_summary: String,
    /// Discovered registry.
    pub verification_mechanisms: Vec<VerificationMechanism>,
    pub divergence_records: Vec<DivergenceRecord>,
    pub findings: Vec<Finding>,
    pub gates: Vec<Gate>,
    pub coverage: CoverageStatement,
    pub technical_assessment: TechnicalAssessment,
    /// FIRA-produced (VR6): restricted to {NOT_APPLICABLE, PENDING}.
    pub human_decision: HumanDecisionFira,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prior_report_ref: Option<String>,
    /// Hash of the structured state fed to the verdict engine (VR7 surface).
    pub determinism_inputs_hash: String,
}

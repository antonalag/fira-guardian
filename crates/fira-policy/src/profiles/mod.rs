//! The five built-in audit profiles (S2) as versioned POLICY data, plus a pure
//! lookup over the closed [`ProfileId`] set.
//!
//! Profiles are **data**: per gate they set a `requirement_level`, plus
//! `focus_areas`, `typical_failure_modes`, and `minimum_evidence_expectations`.
//! They never define how a gate is evaluated (§7) or how the verdict is computed
//! (§11, Task 7) — those live in CORE. A profile selects which gates matter and
//! to what degree; it changes no gate-state or verdict semantics.
//!
//! `library` and `stateful_distributed` reproduce the §20 reference matrix
//! verbatim. `cli_tool`, `web_service`, and `batch_pipeline` are the approved MVP
//! matrices (§20 defines them only "analogously / kept small"). All five are
//! versioned `1.0.0` under contract `1.0-frozen+corr1-4`.

mod batch_pipeline;
mod cli_tool;
mod library;
pub mod selection;
mod stateful_distributed;
mod web_service;

use fira_core::model::{GateName, ProfileId, RequirementLevel};
use serde::{Deserialize, Serialize};

/// A gate entry in a profile (S2 `gates[]`): the gate, its requirement level,
/// and — required where the level is `not_applicable` — a justification stating
/// why the gate is outside the system class's semantics.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProfileGate {
    pub gate: GateName,
    pub requirement_level: RequirementLevel,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub na_justification: Option<String>,
}

/// A minimum-evidence expectation (S2 `minimum_evidence_expectations[]`): what a
/// PASS needs for a gate in this class. The text is the data a later evaluation
/// step reads to populate Task 7's `ExpectationViolations`; POLICY does not
/// evaluate evidence here.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MinimumEvidenceExpectation {
    pub gate: GateName,
    pub expectation: String,
}

/// AuditProfile (S2): versioned POLICY data for one system class.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AuditProfile {
    pub profile_id: ProfileId,
    /// Semver; `1.0.0` for the initial MVP data under contract
    /// `1.0-frozen+corr1-4` (pattern enforced by the S2 schema).
    pub version: String,
    pub description: String,
    pub gates: Vec<ProfileGate>,
    pub focus_areas: Vec<String>,
    pub typical_failure_modes: Vec<String>,
    pub minimum_evidence_expectations: Vec<MinimumEvidenceExpectation>,
}

/// The Semver seeded for every built-in MVP profile.
pub const PROFILE_VERSION: &str = "1.0.0";

/// The built-in profile for an id. Total over the closed [`ProfileId`] enum, so
/// the five profiles are exhaustive by construction.
pub fn profile_for(id: ProfileId) -> AuditProfile {
    match id {
        ProfileId::Library => library::profile(),
        ProfileId::CliTool => cli_tool::profile(),
        ProfileId::WebService => web_service::profile(),
        ProfileId::StatefulDistributed => stateful_distributed::profile(),
        ProfileId::BatchPipeline => batch_pipeline::profile(),
    }
}

/// Every built-in profile id, in `ProfileId` order.
pub const ALL_IDS: [ProfileId; 5] = [
    ProfileId::Library,
    ProfileId::CliTool,
    ProfileId::WebService,
    ProfileId::StatefulDistributed,
    ProfileId::BatchPipeline,
];

/// Every built-in profile, in `ProfileId` order.
pub fn all() -> Vec<AuditProfile> {
    ALL_IDS.into_iter().map(profile_for).collect()
}

/// Small constructors shared by the profile modules, keeping each profile file
/// to its data.
pub(crate) fn required(gate: GateName) -> ProfileGate {
    ProfileGate {
        gate,
        requirement_level: RequirementLevel::Required,
        na_justification: None,
    }
}

pub(crate) fn recommended(gate: GateName) -> ProfileGate {
    ProfileGate {
        gate,
        requirement_level: RequirementLevel::Recommended,
        na_justification: None,
    }
}

pub(crate) fn not_applicable(gate: GateName, justification: &str) -> ProfileGate {
    ProfileGate {
        gate,
        requirement_level: RequirementLevel::NotApplicable,
        na_justification: Some(justification.to_string()),
    }
}

pub(crate) fn expectation(gate: GateName, text: &str) -> MinimumEvidenceExpectation {
    MinimumEvidenceExpectation {
        gate,
        expectation: text.to_string(),
    }
}

pub(crate) fn strings(items: &[&str]) -> Vec<String> {
    items.iter().map(|s| s.to_string()).collect()
}
